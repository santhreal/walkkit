//! Sorting modes for file iteration.

/// Specifies how the discovered files should be sorted.
/// Note that sorting requires collecting all files in memory before yielding them.
///
/// # Examples
///
/// ```rust
/// use walkkit::SortMode;
///
/// let sort_mode = SortMode::ByName;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SortMode {
    /// No sorting; yield files as they are discovered (fastest).
    #[default]
    Unsorted,
    /// Sort files by path name alphabetically.
    ByName,
    /// Sort files by size, ascending.
    BySize,
}
