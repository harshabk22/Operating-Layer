
class TemplateManager:
    def __init__(self):
        self.templates = {
            "welcome": "Welcome {name}!",
            "alert": "Alert: {msg}",
            "approval": "Please approve: {item}"
        }
    def render(self, template_id, context):
        template = self.templates.get(template_id, "")
        return template.format(**context)
