
class InAppChannel:
    def send(self, recipient, content):
        print(f"Sending In-App notification to {recipient}: {content}")
        return True
