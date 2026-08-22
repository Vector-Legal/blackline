//! A1-style cell references.

use crate::error::XlsxError;

/// Column letters → 1-based index (`A` = 1, `Z` = 26, `AA` = 27).
pub fn col_number(letters: &str) -> Result<usize, XlsxError> {
    let mut n = 0usize;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return Err(XlsxError::invalid(format!("invalid column {letters}")));
        }
        n = n * 26 + (b.to_ascii_uppercase() - b'A') as usize + 1;
    }
    if n == 0 {
        return Err(XlsxError::invalid("empty column"));
    }
    Ok(n)
}

/// 1-based column index → letters.
pub fn col_letter(mut num: usize) -> String {
    let mut out = Vec::new();
    while num > 0 {
        num -= 1;
        out.push((b'A' + (num % 26) as u8) as char);
        num /= 26;
    }
    out.reverse();
    out.into_iter().collect()
}

/// Parse `B5` → `(row=5, col=2)`.
pub fn parse_cell_ref(cell: &str) -> Result<(usize, usize), XlsxError> {
    let mut col = String::new();
    let mut row = String::new();
    for ch in cell.chars() {
        if ch.is_ascii_alphabetic() {
            col.push(ch.to_ascii_uppercase());
        } else if ch.is_ascii_digit() {
            row.push(ch);
        } else {
            return Err(XlsxError::invalid(format!("invalid cell {cell}")));
        }
    }
    if col.is_empty() || row.is_empty() {
        return Err(XlsxError::invalid(format!("invalid cell {cell}")));
    }
    let col_n = col_number(&col)?;
    let row_n: usize = row
        .parse()
        .map_err(|_| XlsxError::invalid(format!("invalid row in {cell}")))?;
    Ok((row_n, col_n))
}

/// Format `(row, col)` as `A1`.
pub fn format_cell_ref(row: usize, col: usize) -> String {
    format!("{}{row}", col_letter(col))
}

/// A parsed A1 coordinate: `(row, column)`, both 1-based.
pub type CellCoord = (usize, usize);

/// Parse `A1:C10` → `((1,1), (10,3))`.
pub fn parse_range(range: &str) -> Result<(CellCoord, CellCoord), XlsxError> {
    let (a, b) = range
        .split_once(':')
        .ok_or_else(|| XlsxError::invalid(format!("invalid range {range}")))?;
    Ok((parse_cell_ref(a)?, parse_cell_ref(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters() {
        assert_eq!(col_number("A").unwrap(), 1);
        assert_eq!(col_number("Z").unwrap(), 26);
        assert_eq!(col_number("AA").unwrap(), 27);
        assert_eq!(col_letter(1), "A");
        assert_eq!(col_letter(27), "AA");
        assert_eq!(parse_cell_ref("B5").unwrap(), (5, 2));
        assert_eq!(format_cell_ref(5, 2), "B5");
    }
}
