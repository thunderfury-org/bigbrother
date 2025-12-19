pub use sea_orm_migration::prelude::*;

mod m20251210_105056_create_table_keyword;
mod m20251219_173900_create_table_event;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251210_105056_create_table_keyword::Migration),
            Box::new(m20251219_173900_create_table_event::Migration),
        ]
    }
}
