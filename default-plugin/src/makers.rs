//! Built-in maker types.

use std::net::Ipv4Addr;

use logmaker_plugin_api::{
    ArgSpec, ArgType, Args, Maker, MakerFactory, PluginError, arg_bool, arg_i64, arg_str, arg_string_list,
};
use parking_lot::Mutex;
use rand::RngExt;
use rand_distr::{Distribution, Normal};

use crate::java_date::JavaDateFormat;
use crate::regex_gen;

/// Current time formatted with a Java `SimpleDateFormat` pattern.
pub struct DateFactory;

struct DateMaker {
    format: JavaDateFormat,
}

impl MakerFactory for DateFactory {
    fn type_name(&self) -> &str {
        "Date"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required(
            "format",
            ArgType::String,
            "Java SimpleDateFormat pattern, e.g. yyyy-MM-dd HH:mm:ss.SSS",
        )]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let format = JavaDateFormat::parse(arg_str(args, "format").unwrap_or_default())
            .map_err(|_| PluginError::invalid("format"))?;
        Ok(Box::new(DateMaker { format }))
    }
}

impl Maker for DateMaker {
    fn get_data(&self) -> String {
        self.format.format_now()
    }
}

/// Uniformly random IPv4 address.
pub struct IpFactory;

struct IpMaker;

impl MakerFactory for IpFactory {
    fn type_name(&self) -> &str {
        "IP"
    }

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn create(&self, _name: &str, _args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        Ok(Box::new(IpMaker))
    }
}

impl Maker for IpMaker {
    fn get_data(&self) -> String {
        Ipv4Addr::from(rand::rng().random::<[u8; 4]>()).to_string()
    }
}

/// IPv4 address between `start` and `end`, normally distributed around the
/// middle of the range.
pub struct IpRangeFactory;

struct IpRangeMaker {
    start: u32,
    span: f64,
    distribution: Option<Normal<f64>>,
    mean: f64,
}

impl MakerFactory for IpRangeFactory {
    fn type_name(&self) -> &str {
        "IPRange"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::required("start", ArgType::String, "First IPv4 address of the range."),
            ArgSpec::required("end", ArgType::String, "Last IPv4 address of the range."),
            ArgSpec::required(
                "deviation",
                ArgType::Number,
                "Standard deviation around the middle of the range (capped at a quarter of the range).",
            ),
        ]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let ip = |key: &str| -> Result<u32, PluginError> {
            arg_str(args, key)
                .and_then(|s| s.trim().parse::<Ipv4Addr>().ok())
                .map(u32::from)
                .ok_or_else(|| PluginError::invalid(key))
        };
        let (start, end) = (ip("start")?, ip("end")?);
        if start > end {
            return Err(PluginError::invalid("end"));
        }
        let mean = i64::from((end - start) / 2);
        let mut deviation = arg_i64(args, "deviation").transpose()?.unwrap_or(0).saturating_abs();
        if deviation >= mean {
            deviation = mean / 2;
        }
        Ok(Box::new(IpRangeMaker {
            start,
            span: f64::from(end - start),
            distribution: Normal::new(mean as f64, deviation as f64).ok(),
            mean: mean as f64,
        }))
    }
}

impl Maker for IpRangeMaker {
    fn get_data(&self) -> String {
        let offset = self
            .distribution
            .and_then(|normal| {
                let mut rng = rand::rng();
                (0..64)
                    .map(|_| normal.sample(&mut rng).round())
                    .find(|v| (0.0..=self.span).contains(v))
            })
            .unwrap_or(self.mean);
        Ipv4Addr::from(self.start + offset as u32).to_string()
    }
}

/// Whole number between `start` and `end` (inclusive), random or sequential.
pub struct NumberRangeFactory;

struct NumberRangeMaker {
    start: i64,
    end: i64,
    /// Next value in sequential mode; `None` in random mode.
    next: Option<Mutex<i64>>,
}

impl MakerFactory for NumberRangeFactory {
    fn type_name(&self) -> &str {
        "NumberRange"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::required("start", ArgType::Number, "The smallest number returned."),
            ArgSpec::required("end", ArgType::Number, "The largest number returned."),
            ArgSpec::optional(
                "random",
                ArgType::Boolean,
                "Pick random numbers (default) or count up from start and wrap after end.",
            ),
        ]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let number = |key: &str| arg_i64(args, key).unwrap_or_else(|| Err(PluginError::invalid(key)));
        let (start, end) = (number("start")?, number("end")?);
        let random = arg_bool(args, "random").unwrap_or(true);
        Ok(Box::new(NumberRangeMaker {
            start,
            end,
            next: (!random).then(|| Mutex::new(start)),
        }))
    }
}

impl Maker for NumberRangeMaker {
    fn get_data(&self) -> String {
        let value = match &self.next {
            Some(next) => {
                let mut next = next.lock();
                let value = *next;
                *next = if value >= self.end { self.start } else { value + 1 };
                value
            }
            None if self.start >= self.end => self.start,
            None => rand::rng().random_range(self.start..=self.end),
        };
        value.to_string()
    }
}

/// One of the configured strings, chosen uniformly.
pub struct PickFactory;

struct PickMaker {
    items: Vec<String>,
}

impl MakerFactory for PickFactory {
    fn type_name(&self) -> &str {
        "Pick"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required("picker", ArgType::List, "Values to choose from.")]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        Ok(Box::new(PickMaker {
            items: arg_string_list(args, "picker").unwrap_or_default(),
        }))
    }
}

impl Maker for PickMaker {
    fn get_data(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        self.items[rand::rng().random_range(0..self.items.len())].clone()
    }
}

/// Random string matching a regular expression.
pub struct RegexFactory;

struct RegexMaker {
    regex: rand_regex::Regex,
}

impl MakerFactory for RegexFactory {
    fn type_name(&self) -> &str {
        "Regex"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required(
            "regex",
            ArgType::String,
            "Regular expression the generated values match, e.g. [A-Z]{3}-\\d{4}",
        )]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let regex = regex_gen::compile(arg_str(args, "regex").unwrap_or_default())
            .map_err(|_| PluginError::invalid("regex"))?;
        Ok(Box::new(RegexMaker { regex }))
    }
}

impl Maker for RegexMaker {
    fn get_data(&self) -> String {
        rand::rng().sample(&self.regex)
    }
}

/// Random (version 4) UUID.
pub struct UuidFactory;

struct UuidMaker;

impl MakerFactory for UuidFactory {
    fn type_name(&self) -> &str {
        "UUID"
    }

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn create(&self, _name: &str, _args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        Ok(Box::new(UuidMaker))
    }
}

impl Maker for UuidMaker {
    fn get_data(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use logmaker_plugin_api::check_args;
    use serde_json::{Value, json};

    use super::*;

    fn create(factory: &dyn MakerFactory, args: Value) -> Result<Box<dyn Maker>, PluginError> {
        let args = args.as_object().cloned().unwrap();
        check_args(&factory.args(), &args)?;
        factory.create("test", &args)
    }

    fn values(maker: &dyn Maker, n: usize) -> Vec<String> {
        (0..n).map(|_| maker.get_data()).collect()
    }

    #[test]
    fn ip_maker_emits_valid_addresses() {
        let maker = create(&IpFactory, json!({})).unwrap();
        for v in values(maker.as_ref(), 100) {
            assert!(v.parse::<Ipv4Addr>().is_ok(), "{v}");
        }
    }

    #[test]
    fn ip_range_stays_within_bounds() {
        let maker = create(
            &IpRangeFactory,
            json!({"start": "10.0.0.0", "end": "10.0.1.0", "deviation": 50}),
        )
        .unwrap();
        let (lo, hi) = (
            u32::from(Ipv4Addr::new(10, 0, 0, 0)),
            u32::from(Ipv4Addr::new(10, 0, 1, 0)),
        );
        for v in values(maker.as_ref(), 500) {
            let ip = u32::from(v.parse::<Ipv4Addr>().unwrap());
            assert!((lo..=hi).contains(&ip), "{v}");
        }
        let single = create(
            &IpRangeFactory,
            json!({"start": "1.2.3.4", "end": "1.2.3.4", "deviation": 9}),
        )
        .unwrap();
        assert_eq!(single.get_data(), "1.2.3.4");
    }

    #[test]
    fn ip_range_rejects_bad_addresses() {
        let bad = json!({"start": "10.0.0.300", "end": "10.0.1.0", "deviation": 1});
        assert_eq!(create(&IpRangeFactory, bad).err(), Some(PluginError::invalid("start")));
        let reversed = json!({"start": "10.0.1.0", "end": "10.0.0.0", "deviation": 1});
        assert_eq!(
            create(&IpRangeFactory, reversed).err(),
            Some(PluginError::invalid("end"))
        );
    }

    #[test]
    fn number_range_random_and_sequential() {
        let random = create(&NumberRangeFactory, json!({"start": 5, "end": 7})).unwrap();
        for v in values(random.as_ref(), 100) {
            assert!((5..=7).contains(&v.parse::<i64>().unwrap()));
        }
        let sequential = create(&NumberRangeFactory, json!({"start": 1, "end": 3, "random": false})).unwrap();
        assert_eq!(values(sequential.as_ref(), 7), ["1", "2", "3", "1", "2", "3", "1"]);
        assert!(create(&NumberRangeFactory, json!({"start": 1.5, "end": 3})).is_err());
    }

    #[test]
    fn pick_chooses_configured_values() {
        let maker = create(&PickFactory, json!({"picker": ["a", "b"]})).unwrap();
        assert!(values(maker.as_ref(), 50).iter().all(|v| v == "a" || v == "b"));
        assert!(create(&PickFactory, json!({"picker": []})).is_err());
    }

    #[test]
    fn regex_and_uuid_makers() {
        let maker = create(&RegexFactory, json!({"regex": "[A-Z]{3}-\\d{4}"})).unwrap();
        for v in values(maker.as_ref(), 50) {
            assert_eq!(v.len(), 8, "{v}");
            assert_eq!(&v[3..4], "-");
        }
        assert_eq!(
            create(&RegexFactory, json!({"regex": "("})).err(),
            Some(PluginError::invalid("regex"))
        );

        let uuid = create(&UuidFactory, json!({})).unwrap().get_data();
        assert_eq!(uuid::Uuid::parse_str(&uuid).unwrap().get_version_num(), 4);
    }

    #[test]
    fn date_maker_validates_pattern() {
        let maker = create(&DateFactory, json!({"format": "yyyy"})).unwrap();
        assert_eq!(maker.get_data().len(), 4);
        assert_eq!(
            create(&DateFactory, json!({"format": "yyyy T"})).err(),
            Some(PluginError::invalid("format"))
        );
    }
}
