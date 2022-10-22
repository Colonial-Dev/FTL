use super::dependencies::*;

#[derive(Debug)]
pub enum Dependency {
    Id(String),
    Template(String),
}

// Database write methods
impl Dependency {
    /// Prepares an SQL statement to insert a new row into the `dependencies` table and returns a closure that wraps it.
    pub fn prepare_insert<'a>(
        conn: &'a Connection,
    ) -> Result<impl FnMut(&str, &Dependency) -> Result<()> + 'a> {
        let mut insert_by_id = conn.prepare(
            "
            INSERT OR IGNORE INTO dependencies
            VALUES(?1, ?2);
        ",
        )?;

        let mut insert_by_template = conn.prepare(
            "
            INSERT OR IGNORE INTO dependencies
            VALUES (
                ?1,
                (SELECT id FROM templates WHERE name = ?2)
            )
        ",
        )?;

        let closure = move |page_id: &str, input: &Dependency| {
            match input {
                Dependency::Id(val) => insert_by_id.execute(params![page_id, val])?,
                Dependency::Template(val) => insert_by_template.execute(params![page_id, val])?,
            };

            Ok(())
        };

        Ok(closure)
    }

    /// Prepares an SQL statement to delete all rows in the `dependencies` table with a given page ID,
    /// and returns a closure that wraps it.
    pub fn prepare_sanitize(conn: &Connection) -> Result<impl FnMut(&str) -> Result<()> + '_> {
        let mut stmt = conn.prepare(
            "
            DELETE FROM dependencies WHERE page_id = ?1;
        ",
        )?;

        let closure = move |input: &str| {
            let _ = stmt.execute(params![input])?;
            Ok(())
        };

        Ok(closure)
    }
}
