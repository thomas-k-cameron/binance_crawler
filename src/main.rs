use rusqlite::{params, Connection};
use lazy_static::lazy_static;

lazy_static! {
    static ref DB_DIRECTORY: String = {
        let mut db_path = None;
        for i in  std::env::args() {
            if i.contains("DB_PATH=") {
                db_path.replace(i.replace("DB_PATH=", ""));
            }
        }
    
        if db_path.is_none() {
            println!("You need to specify DB_PATH={{path where you want to save your database files}}");
            #[allow(deprecated)]
            let _ = std::thread::sleep_ms(99999*1000);
            std::process::exit(-1);
        } else {
            db_path.unwrap()
        }
    };
}

#[derive(Copy, Clone, Debug)]
enum BinanceType {
    Future,
    Spot,
}

impl BinanceType {
    fn url(&self) -> &str {
        match self {
            BinanceType::Future => "https://fapi.binance.com/fapi/v1",
            BinanceType::Spot => "https://api.binance.com/api/v3",
        }
    }
    fn exchange_info(&self) -> String {
        format!("{}/{}", self.url(), "exchangeInfo")
    }

    fn klines(&self) -> String {
        format!("{}/{}", self.url(), "klines")
    }

    fn db_path(&self) -> String {
        let db = match self {
            BinanceType::Future => "future.db",
            BinanceType::Spot => "v2.db",
        };
        format!("{}/{}", DB_DIRECTORY.as_str(), db)
    }
}

fn main() {
    println!("{:?}", DB_DIRECTORY.as_str());
    async fn func(bt: BinanceType) {
        loop {
            let db = Connection::open(bt.db_path().to_string().as_str()).unwrap();
            db.pragma_update(None, "wal_checkpoint", &"TRUNCATE".to_string())
                .unwrap();
            async_std::task::block_on(run(bt));
        }
    };

    let a = async_std::task::spawn(func(BinanceType::Future));
    let b = async_std::task::spawn(func(BinanceType::Spot));
    async_std::task::block_on(a);
    async_std::task::block_on(b);
}

async fn run(bt: BinanceType) {
    let db = {
        let sql = include_str!("./schema.sql");
        let db = rusqlite::Connection::open(bt.db_path().as_str()).unwrap();
        db.pragma_update(None, &"JOURNAL_MODE".to_string(), &"WAL".to_string())
            .unwrap();
        db.execute_batch(sql).unwrap();

        db
    };

    let symbols = {
        let resp = ureq::get(bt.exchange_info().as_str()).send_bytes(&[]);
        let s = resp.into_string().unwrap();
        let json: serde_json::Value = serde_json::from_str(s.as_str()).unwrap();
        json.as_object().unwrap()["symbols"]
            .as_array()
            .unwrap()
            .to_owned()
    };

    let symbols = symbols.iter().filter(|i| {
        let status = i.as_object().unwrap().get("status").unwrap();
        "TRADING" == status
    });

    for i in symbols {
        let string = i.as_object().unwrap().get("symbol").unwrap();

        let pair = string.as_str().unwrap();
        let item = db.execute(
            "INSERT INTO data (data_id, pair) VALUES ((SELECT COUNT() FROM data)+1, ?)",
            params![pair],
        );

        match item {
            Err(e) => {
                let check = format!("{:?}", e).contains("UNIQUE constraint failed: data.pair");
                if !check {
                    println!("{:?} {:?}", bt, e);
                }
            }
            _ => (),
        };

        let (max, min) = db
            .query_row(
                r#"
            SELECT 
                MAX(klines.open_time), 
                MIN(klines.open_time)
            FROM klines 
            JOIN data ON data.data_id = klines.data_id 
            WHERE data.pair = ?;
        "#,
                params![pair],
                |i| {
                    let tup: (i64, i64) = match i.get(0) {
                        Ok(item) => (item, i.get(1).unwrap()),
                        _ => return Ok((i64::MAX, i64::MAX)),
                    };
                    Ok(tup)
                },
            )
            .unwrap();

        let range = min..=max;
        println!("{:?} {} {:?}", bt, pair, range);

        let task =
            |time: i64| async_std::task::spawn(crawl(pair.to_string(), time.to_string(), bt));

        let mut time = i64::MAX;
        while let Some(t) = task(time).await {
            println!("{:?}: {:?}", bt, (pair, time));
            if time == t {
                break;
            } else if range.contains(&t) {
                time = *range.start();
            } else {
                time = t;
            }
            let dur = std::time::Duration::from_secs(1);
            async_std::task::sleep(dur).await;
        }
    }

    println!(
        "=======================\n\n\nITERATION ENDED! {:?}\n\n\n=======================\n\n\n",
        bt
    )
}

async fn crawl(symbol: String, end_time: String, bt: BinanceType) -> Option<i64> {
    let (symbol, end_time) = (symbol.as_str(), end_time.as_str());
    let db = rusqlite::Connection::open(bt.db_path().as_str()).unwrap();
    
    let url = bt.klines();
    let resp = {
        ureq::get(url.as_str())
            .query("symbol", symbol.to_uppercase().as_str())
            .query("interval", "1m")
            .query("endTime", end_time)
            .query("limit", "1000")
            .send_bytes(&[])
    };

    if resp.status() != 200 {
        eprintln!("resp: {:?}", resp);
        eprintln!("resp: {:?}", resp.into_string());
        return None;
    }

    let s = match resp.into_string() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{:?}", e);
            return None;
        }
    };

    let arr = match serde_json::from_str(s.as_str()) {
        Ok(serde_json::Value::Array(arr)) => arr,
        e => {
            eprintln!("{:?}", e);
            return None;
        }
    };

    let mut time = i64::MAX;
    let end_time: i64 = end_time.parse().unwrap();
    for i in arr {
        let row = match i {
            serde_json::Value::Array(row) => row,
            _ => break,
        };
        
        {
            let mut sql_values = format! {
                r#"
                    (
                        (
                            SELECT data_id 
                            FROM data 
                            WHERE pair = "{}" 
                            LIMIT 1
                        )
                    ,
                "#, symbol
            };
            for (idx, i) in row.iter().enumerate() {
                let check_idx = idx as i32;
                if [0, 6, 8].contains(&check_idx) {
                    if check_idx == 0 {
                        let i = i.as_i64().unwrap();
                        if time > i {
                            time = i;
                        }
                    }
                    let i = i.as_i64().expect(&idx.to_string()).to_string();
                    sql_values.push_str(i.to_string().as_str());
                    sql_values.push(',');
                } else if idx == 11 {
                    sql_values.pop();
                    sql_values.push(')');
                    break;
                } else {
                    sql_values.push_str(&format!("{:?}", i.as_str().unwrap()));
                    sql_values.push(',');
                }
            }

            if end_time < time {
                break
            }

            let string = format!("INSERT OR IGNORE INTO klines VALUES {};", sql_values);
            db.execute(string.as_str(), params![]).unwrap();
        }
    }
    return time.into();
}
