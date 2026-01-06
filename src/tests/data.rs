use crate::{Connection, Result};

pub(super) fn english(c: &mut Connection) -> Result<()> {
    c.execute(
        r#"
        CREATE TABLE english (value TEXT);

        INSERT INTO english VALUES ('cerotype');
        INSERT INTO english VALUES ('metatype');
        INSERT INTO english VALUES ('ozotype');
        INSERT INTO english VALUES ('phenotype');
        INSERT INTO english VALUES ('plastotype');
        INSERT INTO english VALUES ('undertype');
        INSERT INTO english VALUES ('nonsence');
        "#,
    )?;

    Ok(())
}

pub(super) fn users(c: &mut Connection) -> Result<()> {
    c.execute(
        r#"
        CREATE TABLE users (id INTEGER, name TEXT, age REAL, photo BLOB, email TEXT);

        INSERT INTO users VALUES (1, 'Alice', 42.69, X'4269', NULL);
        "#,
    )?;

    Ok(())
}
