from taskboard.errors import APIError


def create_task(store, payload):
    if not isinstance(payload, dict):
        raise APIError(400, "invalid_payload")
    if set(payload) - {"title", "status"}:
        raise APIError(400, "unknown_field")
    title = payload.get("title")
    if not isinstance(title, str) or not title.strip():
        raise APIError(400, "invalid_title")
    status = payload.get("status", "open")
    if status not in ("open", "done"):
        raise APIError(400, "invalid_status")
    return store.create(title.strip(), status)
