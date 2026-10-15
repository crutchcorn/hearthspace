"""Use the existing owned TouchDevice worker within the Firefox step deadline."""

from .touchpad import TouchDevice


class DeadlineTouchDevice(TouchDevice):
    def __init__(self, session, log, deadline, kind):
        super().__init__(session, log, kind, deadline=deadline)
