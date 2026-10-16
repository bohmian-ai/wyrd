from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    """
    SELECT l.order_id, r.execution_id, r.verdict
    FROM vala.datasets.letters l
    JOIN vala.verification.results r ON r.run_id = l.run_id
    WHERE l.run_id = $1
    """,
    [run.run_id],
)
print(rows.to_polars())
