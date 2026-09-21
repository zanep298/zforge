from taskboard.errors import APIError


def get_task(store, task_id):
    task = store.get(task_id)
    if task is None:
        raise APIError(404, "task_not_found")
    return task
