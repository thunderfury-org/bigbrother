pub use sea_orm_migration::prelude::*;

mod m20251210_105056_create_table_keyword;
mod m20251219_173900_create_table_event;
mod m20260130_000000_create_table_cache;
mod m20260506_000000_create_table_file_index;
mod m20260516_000000_reset_table_file_index;
mod m20260516_120000_create_table_telegram_export_state;
mod m20260522_000000_create_table_import_record;
mod m20260527_000000_add_llm_fields_to_file_description;
mod m20260602_000000_create_table_subscription;
mod m20260602_000001_drop_table_keyword;
mod m20260818_000000_add_subscription_display_fields;
mod m20260824_000000_index_file_location_description_by_description;

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
            Box::new(m20260522_000000_create_table_import_record::Migration),
            Box::new(m20260527_000000_add_llm_fields_to_file_description::Migration),
            Box::new(m20260602_000000_create_table_subscription::Migration),
            Box::new(m20260602_000001_drop_table_keyword::Migration),
            Box::new(m20260818_000000_add_subscription_display_fields::Migration),
            Box::new(m20260824_000000_index_file_location_description_by_description::Migration),
        ]
    }
}
