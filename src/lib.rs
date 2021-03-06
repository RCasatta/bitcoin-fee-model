use crate::fee_bucket::FeeBuckets;
use crate::model_data::ModelData;
use chrono::{DateTime, Datelike, NaiveDateTime, Timelike, Utc};
use std::collections::HashMap;
use std::io::Cursor;

mod error;
mod fee_bucket;
mod matrix;
mod model_data;

pub use error::Error;

pub struct FeeModel {
    low: ModelData,
    high: ModelData,
}

impl FeeModel {
    pub fn new() -> FeeModel {
        let low_model_bytes = include_bytes!("../models/20210217-142524/model.cbor");
        let high_model_bytes = include_bytes!("../models/20210217-154050/model.cbor");
        let low =
            ModelData::from_reader(Cursor::new(low_model_bytes)).expect("checked at test time");
        let high =
            ModelData::from_reader(Cursor::new(high_model_bytes)).expect("checked at test time");
        FeeModel { low, high }
    }

    fn estimate_with_buckets(
        &self,
        block_target: u16,
        timestamp: Option<u32>,
        fee_buckets: &[u64],
        last_block_ts: u32,
    ) -> Result<f32, Error> {
        let mut input = HashMap::new();
        input.insert("confirms_in".to_string(), block_target as f32);

        let utc: DateTime<Utc> = match timestamp {
            Some(timestamp) => {
                let naive = NaiveDateTime::from_timestamp(timestamp as i64, 0);
                DateTime::from_utc(naive, Utc)
            }
            None => Utc::now(),
        };
        let day_of_week = utc.weekday().num_days_from_monday() as f32;
        input.insert("day_of_week".to_string(), day_of_week);
        input.insert("hour".to_string(), utc.hour() as f32);

        let delta = utc.timestamp() - last_block_ts as i64;
        input.insert("delta_last".to_string(), delta as f32);

        for i in 0..=15 {
            input.insert(format!("b{}", i), fee_buckets[i] as f32);
        }

        if block_target <= 2 {
            self.low.norm_predict(&input)
        } else {
            self.high.norm_predict(&input)
        }
    }

    /// compute the fee estimation given the desired `block_target`
    /// `timestamp` if None it's initialized to current time.
    /// `fee_rates` contains the fee rates of transactions in the last 10 blocks, only for transactions
    /// having inputs in this last 10 blocks (so the fee rate is known)
    /// `last_block_ts` last
    pub fn estimate(
        &self,
        block_target: u16,
        timestamp: Option<u32>,
        fee_rates: &[f64],
        last_block_ts: u32,
    ) -> Result<f32, Error> {
        let fee_buckets = FeeBuckets::new(50, 500.0).get(fee_rates);
        self.estimate_with_buckets(block_target, timestamp, &fee_buckets, last_block_ts)
    }
}

#[cfg(test)]
mod tests {
    use crate::model_data::tests::BUCKETS;
    use crate::FeeModel;
    use chrono::{NaiveDate, Utc};

    #[test]
    pub fn test_estimate() {
        let model = FeeModel::new();
        let now = Utc::now().timestamp() as u32;
        let one = model
            .estimate_with_buckets(1, Some(now), &BUCKETS, now - 300)
            .unwrap();
        let two = model
            .estimate_with_buckets(2, Some(now), &BUCKETS, now - 300)
            .unwrap();
        assert!(one > two, "1 block ({}) > 2 ({})", one, two);

        let sunday = NaiveDate::from_ymd(2021, 2, 14)
            .and_hms(12, 0, 0)
            .timestamp() as u32;
        let wednesday = NaiveDate::from_ymd(2021, 2, 17)
            .and_hms(12, 0, 0)
            .timestamp() as u32;
        for i in [1u16, 2, 3, 6, 24, 144].iter() {
            let wed = model
                .estimate_with_buckets(*i, Some(sunday), &BUCKETS, sunday)
                .unwrap();
            let sun = model
                .estimate_with_buckets(*i, Some(wednesday), &BUCKETS, wednesday)
                .unwrap();
            assert!(wed >= sun, "sunday ({}) > wednesday ({})", sun, wed);
        }
    }
}
