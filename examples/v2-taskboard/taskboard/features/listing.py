from taskboard.errors import APIError


def list_tasks(store, query):
    # Status filtering is intentionally left for exercise TB-001.
    if query:
        raise APIError(400, "unsupported_query")
    return {"tasks": store.list()}
