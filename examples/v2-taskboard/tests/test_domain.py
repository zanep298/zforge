import unittest

from taskboard.errors import APIError
from taskboard.features.create import create_task
from taskboard.features.detail import get_task
from taskboard.features.listing import list_tasks
from taskboard.store import TaskStore


class DomainTests(unittest.TestCase):
    def test_empty_store(self):
        self.assertEqual(list_tasks(TaskStore(), {}), {"tasks": []})

    def test_store_snapshots_cannot_mutate_state(self):
        store = TaskStore()
        created = create_task(store, {"title": "Original"})
        created["title"] = "Modified"
        store.list()[0]["title"] = "Modified again"
        store.get(1)["status"] = "done"
        self.assertEqual(get_task(store, 1), {"id": 1, "title": "Original", "status": "open"})

    def test_stores_are_isolated(self):
        first, second = TaskStore(), TaskStore()
        create_task(first, {"title": "Only first"})
        self.assertEqual(second.list(), [])
        self.assertEqual(create_task(second, {"title": "Only second"})["id"], 1)

    def test_rejected_create_does_not_change_store(self):
        store = TaskStore()
        with self.assertRaises(APIError):
            create_task(store, {"title": "Task", "status": None})
        self.assertEqual(store.list(), [])
        self.assertEqual(create_task(store, {"title": "Valid"})["id"], 1)
