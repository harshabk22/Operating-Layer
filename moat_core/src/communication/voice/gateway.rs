use crate::MoatResult;

/// Voice gateway — reserved for future implementation.
/// Voice sessions enter through ingress_events as SystemEvent::VoiceTranscript.
/// Architecture: Voice → VAD → STT → text → ingress_events → runtime (same as text).
pub struct VoiceGateway;

impl Default for VoiceGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceGateway {
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self) -> MoatResult<()> {
        // TODO: Future — VAD, STT, TTS, barge-in, WebRTC
        Ok(())
    }
}

/// Reserved for future voice session handling.
pub struct VoiceSession {
    pub session_id: String,
    pub channel: String,
}

impl VoiceSession {
    pub fn new(session_id: &str, channel: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            channel: channel.to_string(),
        }
    }
}
