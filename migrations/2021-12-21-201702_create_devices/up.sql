CREATE TABLE devices (
  id SERIAL PRIMARY KEY,
  device_id TEXT NOT NULL,
  name TEXT NOT NULL,
  alert BOOLEAN NOT NULL DEFAULT 'f'
)