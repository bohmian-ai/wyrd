use arrow::array::RecordBatch;

pub struct AppendBuffer {
    batches: Vec<RecordBatch>,
    total_rows: usize,
}

impl AppendBuffer {
    pub fn new() -> Self {
        Self {
            batches: Vec::new(),
            total_rows: 0,
        }
    }

    pub fn push(&mut self, batch: RecordBatch) {
        self.total_rows += batch.num_rows();
        self.batches.push(batch);
    }

    pub fn drain(&mut self) -> Vec<RecordBatch> {
        self.total_rows = 0;
        std::mem::take(&mut self.batches)
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
}

impl Default for AppendBuffer {
    fn default() -> Self {
        Self::new()
    }
}
