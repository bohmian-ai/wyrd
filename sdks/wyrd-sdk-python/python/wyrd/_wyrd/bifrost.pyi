"""Native ``wyrd._wyrd.bifrost`` declarations the ``wyrd.bifrost`` facades call.

The declared members are typed exactly. Members not yet declared resolve
through ``__getattr__`` as incomplete, so the facades still import them.
"""

from typing import Any

from ..client import WyrdClient

class TableConfig:
    """Native table declaration the ``wyrd.bifrost.TableConfig`` facade wraps."""

    @staticmethod
    def from_json_schema(
        table: str,
        schema_json: str,
        layout_json: str | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> TableConfig:
        """Build one declaration from a JSON Schema document.

        Args:
            table: the ``namespace.name`` table.
            schema_json: one serialized JSON Schema document.
            layout_json: one serialized ``PhysicalLayoutWire``, if declared.
            compaction_target_file_size_bytes: the requested Forge file target.
            compaction_type: the hyphenated Forge compaction type.
        """
        ...

    @staticmethod
    def from_arrow_ipc(
        table: str,
        schema_ipc: bytes,
        layout_json: str | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> TableConfig:
        """Build one declaration from an Arrow IPC schema.

        Args:
            table: the ``namespace.name`` table.
            schema_ipc: one Arrow IPC-serialized schema.
            layout_json: one serialized ``PhysicalLayoutWire``, if declared.
            compaction_target_file_size_bytes: the requested Forge file target.
            compaction_type: the hyphenated Forge compaction type.
        """
        ...

    @staticmethod
    def describe(table: str, client: WyrdClient | None = None) -> TableConfig:
        """Fetch a registered table's declaration.

        Args:
            table: the ``namespace.name`` table.
            client: the client the read is sent as, or ``None`` for the ambient chain.
        """
        ...

    def __getattr__(self, name: str) -> Any: ...

class Bifrost:
    """Native Bifrost handle both ``wyrd.bifrost`` facades drive."""

    def __init__(
        self,
        table: TableConfig | None = None,
        client: WyrdClient | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> None:
        """Retain the arguments; the handle connects on first use.

        Args:
            table: the active write target, or ``None`` for a query-only client.
            client: the client every call is sent as, or ``None`` for the ambient chain.
            client_byte_limit_bytes: the handle-wide ingestion byte budget.
        """
        ...

    @staticmethod
    def connect(
        table: TableConfig | None = None,
        client: WyrdClient | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> Bifrost:
        """Connect one handle now.

        Args:
            table: the active write target, or ``None`` for a query-only client.
            client: the client every call is sent as, or ``None`` for the ambient chain.
            client_byte_limit_bytes: the handle-wide ingestion byte budget.
        """
        ...

    def alter_table(self, namespace: str, name: str, request_json: str) -> dict[str, Any]:
        """Alter one table at its described version and return the committed description.

        Args:
            namespace: the table's namespace.
            name: the table's name.
            request_json: one serialized ``AlterTableRequest``.
        """
        ...

    def __getattr__(self, name: str) -> Any: ...

def __getattr__(name: str) -> Any: ...
