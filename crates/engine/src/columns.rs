//! Column tables. A table keeps one `Vec` per column so a consumer reads a
//! whole column through one pointer. Unknown values are the per-type
//! sentinel [`Cell::UNKNOWN`]; flags are `u8` (0, 1, or unknown).

/// Element type of a column.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    U8 = 1,
    U16 = 2,
    U32 = 3,
    U64 = 4,
    I16 = 5,
    I32 = 6,
    /// UTF-8 text, NUL-padded to 32 bytes.
    Text32 = 7,
    /// UTF-8 text, NUL-padded to 256 bytes.
    Text256 = 8,
}

impl CellKind {
    /// Bytes of one cell.
    pub const fn bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 => 4,
            Self::U64 => 8,
            Self::Text32 => 32,
            Self::Text256 => 256,
        }
    }
}

/// Fixed-size NUL-padded UTF-8 text cell.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<const N: usize>(pub [u8; N]);

impl<const N: usize> Text<N> {
    /// Copies `bytes` up to the first NUL, cut to fit with a NUL at the end.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut text = [0; N];
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len()).min(N - 1);
        text[..end].copy_from_slice(&bytes[..end]);
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        let end = self.0.iter().position(|byte| *byte == 0).unwrap_or(N);
        std::str::from_utf8(&self.0[..end]).unwrap_or("")
    }
}

pub type Text32 = Text<32>;
pub type Text256 = Text<256>;

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

pub trait Cell: Copy {
    /// Unsigned types use their maximum, signed types their minimum; text is
    /// empty.
    const UNKNOWN: Self;
    const KIND: CellKind;
}

macro_rules! cell {
    ($($ty:ty => $unknown:expr, $kind:ident;)*) => {
        $(impl Cell for $ty {
            const UNKNOWN: Self = $unknown;
            const KIND: CellKind = CellKind::$kind;
        })*
    };
}

cell! {
    u8 => u8::MAX, U8;
    u16 => u16::MAX, U16;
    u32 => u32::MAX, U32;
    u64 => u64::MAX, U64;
    i16 => i16::MIN, I16;
    i32 => i32::MIN, I32;
    Text32 => Text([0; 32]), Text32;
    Text256 => Text([0; 256]), Text256;
}

/// `u8` flag value of a known boolean.
pub const fn flag(value: bool) -> u8 {
    value as u8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: &'static str,
    pub kind: CellKind,
    /// The column's `///` comment in the table definition, one line per
    /// comment line; the bindings and the C ABI reference print it.
    pub doc: &'static str,
}

pub trait Table {
    /// The table's `///` comment in its definition.
    const DOC: &'static str;
    const COLUMNS: &'static [ColumnInfo];
    fn rows(&self) -> usize;
    /// Pointer to the first element of column `index` in [`Self::COLUMNS`].
    fn column_ptr(&self, index: usize) -> *const u8;
    fn clear(&mut self);
    /// Gives every column `rows` rows; rows past the old length are unknown.
    fn set_rows(&mut self, rows: usize);
    /// Writable pointer to the first element of column `index`.
    fn column_mut_ptr(&mut self, index: usize) -> *mut u8;
}

/// Object-safe view of a [`Table`].
pub trait Columns {
    fn rows(&self) -> usize;
    fn column_ptr(&self, index: usize) -> *const u8;
}

impl<T: Table> Columns for T {
    fn rows(&self) -> usize {
        Table::rows(self)
    }

    fn column_ptr(&self, index: usize) -> *const u8 {
        Table::column_ptr(self, index)
    }
}

/// Object-safe writable view of a [`Table`]: every cell type is plain bytes,
/// so a column is filled by copying `rows * kind.bytes()` bytes into it.
pub(crate) trait ColumnsMut {
    fn set_rows(&mut self, rows: usize);
    fn column_mut_ptr(&mut self, index: usize) -> *mut u8;
}

impl<T: Table> ColumnsMut for T {
    fn set_rows(&mut self, rows: usize) {
        Table::set_rows(self, rows)
    }

    fn column_mut_ptr(&mut self, index: usize) -> *mut u8 {
        Table::column_mut_ptr(self, index)
    }
}

/// Declares a column table and its row type: `table!(Units, UnitRow { id: u64, ... })`.
/// The `///` comments of the table and its columns are also kept as
/// [`Table::DOC`] and [`ColumnInfo::doc`], so the generated bindings carry them.
macro_rules! table {
    ($(#[doc = $doc:literal])* $name:ident, $row:ident { $($(#[doc = $cdoc:literal])* $col:ident: $ty:ty,)+ }) => {
        $(#[doc = $doc])*
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct $name {
            $($(#[doc = $cdoc])* pub $col: Vec<$ty>,)+
        }

        /// One row of
        #[doc = concat!("[`", stringify!($name), "`].")]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $row {
            $($(#[doc = $cdoc])* pub $col: $ty,)+
        }

        impl $row {
            /// A row with every column unknown.
            pub const UNKNOWN: Self = Self { $($col: <$ty as $crate::columns::Cell>::UNKNOWN,)+ };
        }

        impl $name {
            pub fn push(&mut self, row: $row) {
                $(self.$col.push(row.$col);)+
            }

            pub fn row(&self, index: usize) -> Option<$row> {
                Some($row { $($col: *self.$col.get(index)?,)+ })
            }
        }

        impl $crate::columns::Table for $name {
            const DOC: &'static str = concat!($($doc, "\n",)*);
            const COLUMNS: &'static [$crate::columns::ColumnInfo] = &[
                $($crate::columns::ColumnInfo {
                    name: stringify!($col),
                    kind: <$ty as $crate::columns::Cell>::KIND,
                    doc: concat!($($cdoc, "\n",)*),
                },)+
            ];

            fn rows(&self) -> usize {
                [$(self.$col.len(),)+][0]
            }

            fn column_ptr(&self, index: usize) -> *const u8 {
                [$(self.$col.as_ptr().cast::<u8>(),)+][index]
            }

            fn clear(&mut self) {
                $(self.$col.clear();)+
            }

            fn set_rows(&mut self, rows: usize) {
                $(self.$col.resize(rows, <$ty as $crate::columns::Cell>::UNKNOWN);)+
            }

            fn column_mut_ptr(&mut self, index: usize) -> *mut u8 {
                [$(self.$col.as_mut_ptr().cast::<u8>(),)+][index]
            }
        }
    };
}

pub(crate) use table;
