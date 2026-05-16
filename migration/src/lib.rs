pub use sea_orm_migration::prelude::*;

mod m20251210_105056_create_table_keyword;
mod m20251219_173900_create_table_event;
mod m20260130_000000_create_table_cache;
mod m20260506_000000_create_table_file_index;
mod m20260516_000000_reset_table_file_index;
mod m20260516_120000_create_table_telegram_export_state;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251210_105056_create_table_keyword::Migration),
            Box::new(m20251219_173900_create_table_event::Migration),
            Box::new(m20260130_000000_create_table_cache::Migration),
            Box::new(m20260506_000000_create_table_file_index::Migration),
            Box::new(m20260516_000000_reset_table_file_index::Migration),
            Box::new(m20260516_120000_create_table_telegram_export_state::Migration),
        ]
    }
}
