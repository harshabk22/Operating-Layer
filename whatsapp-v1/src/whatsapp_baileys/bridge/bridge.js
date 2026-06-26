import { makeWASocket, useMultiFileAuthState, DisconnectReason, fetchLatestBaileysVersion } from '@whiskeysockets/baileys';
import express from 'express';
import pino from 'pino';
import path from 'path';
import { existsSync, mkdirSync, readFileSync } from 'fs';
import QRCode from 'qrcode';

// Parse CLI args
const args = process.argv.slice(2);
function getArg(name, defaultVal) {
  const idx = args.indexOf(`--${name}`);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : defaultVal;
}

const PORT = parseInt(getArg('port', '3003'), 10);
const SESSION_DIR = getArg('session', './session');

mkdirSync(SESSION_DIR, { recursive: true });

const logger = pino({ level: 'silent' });
const messageQueue = [];
const MAX_QUEUE_SIZE = 500;

let sock = null;
let connectionState = 'unpaired'; // unpaired, pairing, connected, reconnecting, disconnected
let latestQr = null;

async function startSocket() {
  const { state, saveCreds } = await useMultiFileAuthState(SESSION_DIR);
  let version;
  try {
    const v = await fetchLatestBaileysVersion();
    version = v.version;
  } catch (e) {
    version = [6, 33, 0];
  }

  // Check if we already have credentials
  const credsExist = existsSync(path.join(SESSION_DIR, 'creds.json'));
  if (credsExist && connectionState === 'unpaired') {
    connectionState = 'disconnected';
  }

  sock = makeWASocket({
    version,
    auth: state,
    logger,
    printQRInTerminal: true,
    browser: ['Moat Personal WhatsApp', 'Chrome', '120.0'],
    syncFullHistory: false,
    markOnlineOnConnect: true,
  });

  sock.ev.on('creds.update', saveCreds);

  sock.ev.on('connection.update', async (update) => {
    const { connection, lastDisconnect, qr } = update;

    if (qr) {
      connectionState = 'pairing';
      try {
        latestQr = await QRCode.toDataURL(qr);
      } catch (err) {
        console.error('Failed to generate QR base64:', err);
      }
    }

    if (connection === 'close') {
      const reason = lastDisconnect?.error?.output?.statusCode;
      latestQr = null;

      if (reason === DisconnectReason.loggedOut) {
        connectionState = 'unpaired';
        console.log('Session logged out. Transitioning to unpaired.');
      } else {
        connectionState = 'reconnecting';
        console.log(`Connection closed (reason: ${reason}). Reconnecting in 3s...`);
        setTimeout(startSocket, 3000);
      }
    } else if (connection === 'open') {
      connectionState = 'connected';
      latestQr = null;
      console.log('WhatsApp connected successfully!');
    }
  });

  sock.ev.on('messages.upsert', async ({ messages, type }) => {
    if (type !== 'notify') return;

    for (const msg of messages) {
      if (!msg.message || msg.key.fromMe) continue;

      const chatId = msg.key.remoteJid;
      const senderId = msg.key.participant || chatId;
      const isGroup = chatId.endsWith('@g.us');

      // Skip groups / status messages
      if (isGroup || chatId.includes('status')) continue;

      let body = '';
      const content = msg.message;
      
      if (content.conversation) {
        body = content.conversation;
      } else if (content.extendedTextMessage?.text) {
        body = content.extendedTextMessage.text;
      } else if (content.imageMessage?.caption) {
        body = content.imageMessage.caption;
      } else if (content.videoMessage?.caption) {
        body = content.videoMessage.caption;
      } else if (content.documentMessage?.caption) {
        body = content.documentMessage.caption;
      }

      // Fallback if message has media but no caption
      if (!body) {
        if (content.imageMessage) body = '[Image]';
        else if (content.videoMessage) body = '[Video]';
        else if (content.audioMessage) body = '[Audio]';
        else if (content.documentMessage) body = '[Document]';
      }

      if (!body) continue;

      const event = {
        messageId: msg.key.id,
        chatId,
        senderId,
        senderName: msg.pushName || senderId.split('@')[0],
        body,
        timestamp: msg.messageTimestamp,
      };

      messageQueue.push(event);
      if (messageQueue.length > MAX_QUEUE_SIZE) {
        messageQueue.shift();
      }
    }
  });
}

// Set up Express Server
const app = express();
app.use(express.json());

// Endpoints
app.get('/health', (req, res) => {
  res.json({
    status: connectionState,
    uptime: process.uptime(),
  });
});

app.get('/qr', (req, res) => {
  res.json({
    qr: latestQr,
  });
});

app.get('/messages', (req, res) => {
  const msgs = messageQueue.splice(0, messageQueue.length);
  res.json(msgs);
});

app.post('/send', async (req, res) => {
  if (!sock || connectionState !== 'connected') {
    return res.status(503).json({ error: 'Not connected to WhatsApp' });
  }

  const { chatId, message } = req.body;
  if (!chatId || !message) {
    return res.status(400).json({ error: 'chatId and message are required' });
  }

  try {
    const formattedChatId = chatId.includes('@') ? chatId : `${chatId.replace('+', '')}@s.whatsapp.net`;
    const sent = await sock.sendMessage(formattedChatId, { text: message });
    res.json({
      success: true,
      messageId: sent.key.id,
    });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

// Start Express and WhatsApp socket
app.listen(PORT, '127.0.0.1', () => {
  console.log(`WhatsApp Baileys Bridge running on port ${PORT}`);
  startSocket().catch((err) => {
    console.error('Failed to start WhatsApp socket:', err);
  });
});
