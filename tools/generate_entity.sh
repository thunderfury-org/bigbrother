export DATABASE_URL=sqlite:data/db/data.db?mode=rwc
sea-orm-cli generate entity \
  -o ./src/infrastructure/entity/model \
  --with-prelude none \
  --entity-format dense
