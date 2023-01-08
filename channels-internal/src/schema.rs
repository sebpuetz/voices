// @generated automatically by Diesel CLI.

diesel::table! {
    channel_client_state (client_id, channel_id) {
        client_id -> Uuid,
        channel_id -> Uuid,
    }
}

diesel::table! {
    channels (id) {
        id -> Uuid,
        server_id -> Uuid,
        assigned_to -> Nullable<Uuid>,
        name -> Text,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    servers (id) {
        id -> Uuid,
        name -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    servers_members (client_id, server_id) {
        client_id -> Uuid,
        server_id -> Uuid,
    }
}

diesel::table! {
    voice_servers (id) {
        id -> Uuid,
        host -> Text,
    }
}

diesel::joinable!(channel_client_state -> channels (channel_id));
diesel::joinable!(channels -> servers (server_id));
diesel::joinable!(channels -> voice_servers (assigned_to));
diesel::joinable!(servers_members -> servers (server_id));

diesel::allow_tables_to_appear_in_same_query!(
    channel_client_state,
    channels,
    servers,
    servers_members,
    voice_servers,
);
