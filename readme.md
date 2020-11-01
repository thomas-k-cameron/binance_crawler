# About
Crawler for collecting binance's historical data.
Collected data will be stored in sqlite.

# How To
```
./%directory%/binance_crawler.exe DB_PATH=$Directory Where you want to create or have the database file
```

# Schema
```sql
CREATE TABLE IF NOT EXISTS data (
    data_id INTEGER AUTO_INCREMENT NOT NULL PRIMARY KEY,
    pair TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS klines (
    data_id BIGINT UNSIGNED NOT NULL,
    open_time BIGINT UNSIGNED NOT NULL,
    open DECIMAL(28, 10) NOT NULL,
    high DECIMAL(28, 10) NOT NULL,
    low DECIMAL(28, 10) NOT NULL,
    close DECIMAL(28, 10) NOT NULL,
    volume DECIMAL(28, 10) NOT NULL,
    close_time BIGINT UNSIGNED NOT NULL,
    quote_asset_volume DECIMAL(28, 10) NOT NULL,
    trades BIGINT UNSIGNED NOT NULL,
    taker DECIMAL(28, 10) NOT NULL,
    quote DECIMAL(28, 10) NOT NULL,

    PRIMARY KEY(data_id, open_time),
    FOREIGN KEY (data_id) REFERENCES data(data_id)
);
```

# Examples
This gives you a historical 1 min ohlc data for BTCUSDT from the database file.
```
SELECT * FROM klines k
    JOIN data d 
    ON k.data_id = d.data_id 
    WHERE d.pair = "BTCUSDT"
    ORDER BY k.open_time DESC
    LIMIT 1000
```
