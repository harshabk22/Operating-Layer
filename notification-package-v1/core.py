
from .channels.email import EmailChannel
from .channels.whatsapp import WhatsAppChannel
from .channels.in_app import InAppChannel
from .features.templates import TemplateManager
from .features.escalations import EscalationManager
from .features.approvals import ApprovalManager

class NotificationManager:
    def __init__(self):
        self.channels = {
            "email": EmailChannel(),
            "whatsapp": WhatsAppChannel(),
            "in_app": InAppChannel()
        }
        self.templates = TemplateManager()
        self.escalations = EscalationManager()
        self.approvals = ApprovalManager()

    def send(self, channel, recipient, template_id, context):
        content = self.templates.render(template_id, context)
        return self.channels[channel].send(recipient, content)

    def send_approval(self, channel, recipient, approval_data):
        return self.approvals.request_approval(self, channel, recipient, approval_data)
