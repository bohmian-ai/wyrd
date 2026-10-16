# Python examples

Run these examples from a Wyrd checkout after installing the Python development
environment:

```bash
mise run py:setup
mise run examples:python:datacard
```

Expected output:

```text
saved_card_json: true
saved_data_file: data/data.parquet
loaded_rows: 3
loaded_columns: customer_id,churned,segment
labels: domain=customer,stage=example
annotation_source: examples/python/datacard_local_workflow.py
```

The DataCard example uses pandas because it is part of the Wyrd Python
development setup. It writes into a temporary directory and removes the local
files before exiting.
