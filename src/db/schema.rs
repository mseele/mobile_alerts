table! {
    devices (id) {
        id -> Int4,
        device_id -> Text,
        name -> Text,
        alert -> Bool,
    }
}

table! {
    measurements (id) {
        id -> Int4,
        device_id -> Int4,
        time -> Timestamptz,
        temperature -> Float8,
        humidity -> Float8,
        temperature_outside -> Nullable<Float8>,
        humidity_outside -> Nullable<Float8>,
    }
}

joinable!(measurements -> devices (device_id));

allow_tables_to_appear_in_same_query!(
    devices,
    measurements,
);
