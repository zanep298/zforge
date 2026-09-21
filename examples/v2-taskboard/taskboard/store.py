from threading import Lock


class TaskStore:
    """Process-local synthetic data. A new process starts empty."""

    def __init__(self):
        self._lock = Lock()
        self._tasks = {}
        self._next_id = 1

    def create(self, title, status):
        with self._lock:
            task = {"id": self._next_id, "title": title, "status": status}
            self._tasks[task["id"]] = task
            self._next_id += 1
            return dict(task)

    def list(self):
        with self._lock:
            return [dict(task) for task in self._tasks.values()]

    def get(self, task_id):
        with self._lock:
            task = self._tasks.get(task_id)
            return dict(task) if task is not None else None
