"""Write the Arrow fixtures the every-Iceberg-column-type story reads in every SDK.

``every_iceberg_type.arrow`` holds one column of every Iceberg type in its
Arrow form. Row 1 holds ordinary values, row 2 is null in every column, row 3
keeps each parent valid around a null child, and row 4 holds extremes and
empty collections.

``narrow_types.arrow`` holds narrower Arrow spellings that Bifrost stores as
their wider Iceberg type.

Regenerate both after editing this file:

    cd sdks/wyrd-sdk-python && uv run python ../../fixtures/bifrost/every_iceberg_type.py
"""

from __future__ import annotations

import uuid
from decimal import Decimal
from pathlib import Path

import pyarrow
import pyarrow.ipc

HERE = Path(__file__).parent
ELEMENT = "element"

EVERY_TYPE = pyarrow.RecordBatch.from_pydict(
    {
        "id": pyarrow.array([1, 2, 3, 4], pyarrow.int64()),
        "flag": pyarrow.array([True, None, False, True]),
        "int32": pyarrow.array([7, None, 0, 2**31 - 1], pyarrow.int32()),
        "int64": pyarrow.array([7, None, 0, 2**63 - 1], pyarrow.int64()),
        "float32": pyarrow.array([1.5, None, -0.25, 3.4028234663852886e38], pyarrow.float32()),
        "float64": pyarrow.array([1.5, None, -0.25, 1.7976931348623157e308], pyarrow.float64()),
        "decimal": pyarrow.array(
            [Decimal("123456789.000000001"), None, Decimal("-0.000000001"), Decimal(0)],
            pyarrow.decimal128(38, 9),
        ),
        "date": pyarrow.array([19_000, None, 0, -1], pyarrow.int32()).cast(pyarrow.date32()),
        "time": pyarrow.array([1, None, 0, 86_399_999_999], pyarrow.time64("us")),
        "timestamp": pyarrow.array([1, None, 0, -1], pyarrow.timestamp("us")),
        "timestamp_utc": pyarrow.array([1, None, 0, -1], pyarrow.timestamp("us", tz="+00:00")),
        "timestamp_ns": pyarrow.array([1, None, 0, -1], pyarrow.timestamp("ns")),
        "timestamp_ns_utc": pyarrow.array([1, None, 0, -1], pyarrow.timestamp("ns", tz="+00:00")),
        "text": pyarrow.array(["héllo", None, "", "z"]),
        "bytes": pyarrow.array([b"\x00\xff", None, b"", b"\x01"], pyarrow.large_binary()),
        "uuid": pyarrow.array(
            [uuid.UUID(int=1).bytes, None, uuid.UUID(int=0).bytes, uuid.UUID(int=2**128 - 1).bytes],
            pyarrow.binary(16),
        ),
        "fixed": pyarrow.array([b"abc", None, b"\x00\x00\x00", b"\xff\xff\xff"], pyarrow.binary(3)),
        "numbers": pyarrow.array(
            [[1, 2], None, [None, 3], []], pyarrow.list_(pyarrow.field(ELEMENT, pyarrow.int64()))
        ),
        "point": pyarrow.array(
            [{"x": 1, "label": "a"}, None, {"x": None, "label": "c"}, {"x": 0, "label": None}],
            pyarrow.struct([("x", pyarrow.int64()), ("label", pyarrow.string())]),
        ),
        "nested": pyarrow.array(
            [
                {"inner": {"v": 7}, "tags": ["t"]},
                None,
                {"inner": None, "tags": None},
                {"inner": {"v": None}, "tags": []},
            ],
            pyarrow.struct(
                [
                    ("inner", pyarrow.struct([("v", pyarrow.int64())])),
                    ("tags", pyarrow.list_(pyarrow.field(ELEMENT, pyarrow.string()))),
                ]
            ),
        ),
        "counts": pyarrow.array(
            [[("a", 1), ("b", 2)], None, [("k", None)], []],
            pyarrow.map_(pyarrow.string(), pyarrow.int64()),
        ),
        "records": pyarrow.array(
            [[("x", {"a": 1}), ("y", {"a": None})], None, [("z", None)], []],
            pyarrow.map_(pyarrow.string(), pyarrow.struct([("a", pyarrow.int64())])),
        ),
    }
)

NARROW = pyarrow.RecordBatch.from_pydict(
    {
        "int8": pyarrow.array([-128, None], pyarrow.int8()),
        "uint16": pyarrow.array([65_535, None], pyarrow.uint16()),
        "uint32": pyarrow.array([4_294_967_295, None], pyarrow.uint32()),
        "utc": pyarrow.array([1_700_000_000, None], pyarrow.timestamp("us", tz="UTC")),
        "large_text": pyarrow.array(["wide", None], pyarrow.large_string()),
        "binary": pyarrow.array([b"\x00\xff", None], pyarrow.binary()),
        "int16_list": pyarrow.array([[-7, None], None], pyarrow.list_(pyarrow.int16())),
    }
)


def write(batch: pyarrow.RecordBatch, name: str) -> None:
    with pyarrow.ipc.new_file(HERE / name, batch.schema) as writer:
        writer.write_batch(batch)


if __name__ == "__main__":
    write(EVERY_TYPE, "every_iceberg_type.arrow")
    write(NARROW, "narrow_types.arrow")
