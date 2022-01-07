CREATE TABLE measurements (
  id SERIAL PRIMARY KEY,
  device_id INTEGER NOT NULL REFERENCES devices (id),
  time TIMESTAMP WITH TIME ZONE NOT NULL,
  temperature FLOAT NOT NULL,
  humidity FLOAT NOT NULL,
  temperature_outside FLOAT NULL,
  humidity_outside FLOAT NULL
)