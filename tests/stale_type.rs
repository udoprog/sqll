//! Type tokens such as `ty::Text` and `ty::Blob` are public, so safe code can
//! check a column on one statement or row and use the token with another. This
//! must never read out of bounds or through a null pointer, and must never
//! convert a column in a way that invalidates previously borrowed values.

use sqll::ty::{self, Type};
use sqll::{Code, Connection, FromUnsizedColumn, Result, Text};

const LONG: &str =
    "this is a long piece of text which is used to produce a stale column length token";

#[test]
fn text_token_on_null_column() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let mut a = c.prepare(format!("SELECT '{LONG}'"))?;
    assert!(a.step()?.is_row());
    let token = ty::Text::check(&mut a, 0.into())?;
    assert_eq!(token.len(), LONG.len());

    let mut b = c.prepare("SELECT NULL")?;
    assert!(b.step()?.is_row());

    let e = Text::from_unsized_column(&b, token).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);
    Ok(())
}

#[test]
fn text_token_on_shorter_column() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let mut a = c.prepare(format!("SELECT '{LONG}'"))?;
    assert!(a.step()?.is_row());
    let token = ty::Text::check(&mut a, 0.into())?;

    let mut b = c.prepare("SELECT 'ab'")?;
    assert!(b.step()?.is_row());

    assert_eq!(<str>::from_unsized_column(&b, token)?, "ab");
    Ok(())
}

#[test]
fn text_token_after_step() -> Result<()> {
    let c = Connection::open_in_memory()?;

    c.execute(format!(
        "CREATE TABLE t (v); INSERT INTO t (v) VALUES ('{LONG}'), ('x'), (NULL), (X'0102');"
    ))?;

    let mut stmt = c.prepare("SELECT v FROM t ORDER BY rowid")?;
    assert!(stmt.step()?.is_row());
    let token = ty::Text::check(&mut stmt, 0.into())?;
    let token2 = ty::Text::check(&mut stmt, 0.into())?;
    let token3 = ty::Text::check(&mut stmt, 0.into())?;

    assert!(stmt.step()?.is_row());
    assert_eq!(<str>::from_unsized_column(&stmt, token)?, "x");

    assert!(stmt.step()?.is_row());
    let e = <str>::from_unsized_column(&stmt, token2).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);

    assert!(stmt.step()?.is_row());
    let e = <str>::from_unsized_column(&stmt, token3).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);

    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn blob_token_on_other_statement() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let mut a = c.prepare("SELECT X'0102030405060708090a0b0c0d0e0f10'")?;
    assert!(a.step()?.is_row());
    let null_token = ty::Blob::check(&mut a, 0.into())?;
    let short_token = ty::Blob::check(&mut a, 0.into())?;
    let text_token = ty::Blob::check(&mut a, 0.into())?;
    assert_eq!(null_token.len(), 16);

    let mut b = c.prepare("SELECT NULL, X'01', 'text'")?;
    assert!(b.step()?.is_row());

    let e = <[u8]>::from_unsized_column(&b, null_token).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);

    // The stale token still refers to column 0, which is NULL in `b`.
    let e = <[u8]>::from_unsized_column(&b, short_token).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);

    let mut b = c.prepare("SELECT X'01'")?;
    assert!(b.step()?.is_row());
    assert_eq!(<[u8]>::from_unsized_column(&b, text_token)?, b"\x01");
    Ok(())
}

#[test]
fn token_for_missing_column() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let mut a = c.prepare("SELECT 1, 2, 'third'")?;
    assert!(a.step()?.is_row());
    let token = ty::Text::check(&mut a, 2.into())?;

    let mut b = c.prepare("SELECT 'only'")?;
    assert!(b.step()?.is_row());

    let e = <str>::from_unsized_column(&b, token).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);
    Ok(())
}

/// Reading a BLOB column as TEXT would make SQLite convert it in place, which
/// can reallocate it and invalidate a slice already borrowed from the column.
#[test]
fn stale_text_token_does_not_convert_blob() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let mut a = c.prepare("SELECT 'text'")?;
    assert!(a.step()?.is_row());
    let text_token = ty::Text::check(&mut a, 0.into())?;

    let mut b = c.prepare("SELECT X'0102030405060708'")?;
    assert!(b.step()?.is_row());
    let blob_token = ty::Blob::check(&mut b, 0.into())?;

    let bytes = <[u8]>::from_unsized_column(&b, blob_token)?;
    let e = Text::from_unsized_column(&b, text_token).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);
    assert_eq!(bytes, b"\x01\x02\x03\x04\x05\x06\x07\x08");
    Ok(())
}
