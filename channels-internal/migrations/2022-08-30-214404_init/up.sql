-- Your SQL goes here
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE "servers" (
  "id" UUID PRIMARY KEY DEFAULT (uuid_generate_v4()),
  "name" TEXT NOT NULL,
  "created_at" timestamptz NOT NULL DEFAULT (NOW()),
  "updated_at" timestamptz NOT NULL DEFAULT (NOW())
);

CREATE TABLE "voice_servers" (
  "id" UUID PRIMARY KEY DEFAULT (uuid_generate_v4()),
  "host" TEXT NOT NULL UNIQUE,
  "count" int4 DEFAULT (0)
);

CREATE TABLE "channels" (
  "id" UUID PRIMARY KEY DEFAULT (uuid_generate_v4()),
  "server_id" UUID NOT NULL REFERENCES "servers"("id"),
  "assigned_to" UUID REFERENCES "voice_servers"("id"),
  "name" TEXT NOT NULL,
  "updated_at" timestamptz NOT NULL DEFAULT (NOW())
);

CREATE TABLE "channel_client_state" (
  "client_id" UUID NOT NULL,
  "channel_id" UUID NOT NULL REFERENCES "channels"("id"),
  PRIMARY KEY ("client_id", "channel_id")
);


CREATE TABLE "servers_members" (
  "client_id" UUID NOT NULL,
  "server_id" UUID NOT NULL REFERENCES "servers"("id"),
  PRIMARY KEY ("client_id", "server_id")
);


CREATE INDEX idx_channels_server_id ON channels ("server_id");
CREATE INDEX idx_servers_members_client_id ON servers_members ("client_id");
CREATE INDEX idx_servers_members_server_id ON servers_members ("server_id");

SELECT diesel_manage_updated_at('servers');
SELECT diesel_manage_updated_at('servers_members');
SELECT diesel_manage_updated_at('channels');

