use crate::integrability_utils::SampleTable;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasisStats {
    pub ncols: usize,
    pub dim: usize,
    pub rank: usize,
    pub rows_attempted: usize,
    pub rows_inserted: usize,
    pub vars_count: usize,
    pub envs_total: usize,
    pub samples_used: usize,
    pub rows_skipped_singular: usize,
    pub constraints_insufficient_samples: usize,
    pub max_row_nnz: usize,
    pub sum_row_nnz: usize,
    pub dlog_cache_hits: u64,
    pub dlog_cache_misses: u64,
    pub wedge_cache_hits: u64,
    pub wedge_cache_misses: u64,
    pub sample_table: SampleTable,
}

impl BasisStats {
    pub fn one_line(&self) -> String {
        let avg_row_nnz = if self.rows_inserted == 0 {
            0
        } else {
            self.sum_row_nnz / self.rows_inserted
        };
        format!(
            "ncols={}, dim={}, rank={}, rows_attempted={}, rows_inserted={}, samples_used={}, envs_total={}, sample_table={}, rows_skipped_singular={}, constraints_insufficient_samples={}, vars={}, max_row_nnz={}, avg_row_nnz={}",
            self.ncols,
            self.dim,
            self.rank,
            self.rows_attempted,
            self.rows_inserted,
            self.samples_used,
            self.envs_total,
            self.sample_table.as_str(),
            self.rows_skipped_singular,
            self.constraints_insufficient_samples,
            self.vars_count,
            self.max_row_nnz,
            avg_row_nnz
        )
    }
}
