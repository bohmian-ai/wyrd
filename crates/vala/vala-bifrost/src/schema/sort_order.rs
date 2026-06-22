use iceberg::spec::SortOrder;

pub fn default_bifrost_sort_order() -> SortOrder {
    SortOrder::unsorted_order()
}
