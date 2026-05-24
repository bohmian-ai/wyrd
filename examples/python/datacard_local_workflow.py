from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory

import pandas as pd
from wyrd.data import DataCard, PandasInterface


def main() -> None:
    data = pd.DataFrame(
        {
            "customer_id": [1001, 1002, 1003],
            "churned": [False, True, False],
            "segment": ["team", "enterprise", "team"],
        }
    )

    with TemporaryDirectory() as tmp:
        path = Path(tmp) / "customer_churn_card"
        card = DataCard(
            PandasInterface(data=data),
            name="customer-churn",
            version="0.1.0",
            labels={"domain": "customer", "stage": "example"},
            annotations={"example.com/source": "examples/python/datacard_local_workflow.py"},
        )

        card.save(path)

        restored = DataCard.model_validate_json((path / "card.json").read_text())
        restored.load(path)

        saved_data_file = (path / "data" / "data.parquet").relative_to(path)
        labels = restored.labels
        annotations = restored.annotations

        print(f"saved_card_json: {str((path / 'card.json').exists()).lower()}")
        print(f"saved_data_file: {saved_data_file.as_posix()}")
        print(f"loaded_rows: {len(restored.data)}")
        print(f"loaded_columns: {','.join(restored.data.columns)}")
        print(f"labels: domain={labels['domain']},stage={labels['stage']}")
        print(f"annotation_source: {annotations['example.com/source']}")


if __name__ == "__main__":
    main()
