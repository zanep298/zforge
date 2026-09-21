class APIError(Exception):
    def __init__(self, status, code):
        self.status = status
        self.code = code
        super().__init__(code)
