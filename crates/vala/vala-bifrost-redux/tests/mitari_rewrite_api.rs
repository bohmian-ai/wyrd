use iceberg::Catalog;
use iceberg::spec::DataFile;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};

async fn rewrite_files_api_probe(
    table: Table,
    catalog: &dyn Catalog,
    delete_paths: Vec<String>,
    additions: Vec<DataFile>,
) -> iceberg::Result<()> {
    let tx = Transaction::new(&table);
    let tx = tx
        .rewrite_files()
        .delete_files(delete_paths)
        .add_data_files(additions)
        .apply(tx)?;

    tx.commit(catalog).await.map(|_| ())
}

#[test]
fn mitari_rewrite_action_api_compiles() {
    let _ = rewrite_files_api_probe;
}
