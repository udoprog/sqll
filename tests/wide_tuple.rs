use anyhow::Result;
use sqll::Connection;

#[test]
fn test_32_element_tuple() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute("CREATE TABLE t (c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER, c4 INTEGER, c5 INTEGER, c6 INTEGER, c7 INTEGER, c8 INTEGER, c9 INTEGER, c10 INTEGER, c11 INTEGER, c12 INTEGER, c13 INTEGER, c14 INTEGER, c15 INTEGER, c16 INTEGER, c17 INTEGER, c18 INTEGER, c19 INTEGER, c20 INTEGER, c21 INTEGER, c22 INTEGER, c23 INTEGER, c24 INTEGER, c25 INTEGER, c26 INTEGER, c27 INTEGER, c28 INTEGER, c29 INTEGER, c30 INTEGER, c31 INTEGER)")?;

    let mut stmt = c.prepare("INSERT INTO t VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
    stmt.execute((
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31,
    ))?;

    let mut stmt = c.prepare("SELECT * FROM t")?;
    let row = stmt.next::<(
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    )>()?;
    let row = row.expect("row");
    assert_eq!(row.0, 0);
    assert_eq!(row.31, 31);

    Ok(())
}
