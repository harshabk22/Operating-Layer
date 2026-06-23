
class ApprovalManager:
    def request_approval(self, notification_manager, channel, recipient, approval_data):
        content = notification_manager.templates.render("approval", {"item": approval_data})
        return notification_manager.channels[channel].send(recipient, content)
