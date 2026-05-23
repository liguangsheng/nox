use std::rc::{Rc, Weak};

use crate::{
    Array, EnumValue, Function, JsonValue, Map, OptionValue, Record, ResultValue, Tuple, Value,
};

#[derive(Debug, Default)]
pub(crate) struct GcHeap {
    strings: Vec<Weak<str>>,
    jsons: Vec<Weak<JsonValue>>,
    arrays: Vec<Weak<Array>>,
    tuples: Vec<Weak<Tuple>>,
    maps: Vec<Weak<Map>>,
    options: Vec<Weak<OptionValue>>,
    results: Vec<Weak<ResultValue>>,
    enums: Vec<Weak<EnumValue>>,
    records: Vec<Weak<Record>>,
    functions: Vec<Weak<Function>>,
}

impl GcHeap {
    pub(crate) fn alloc_string(&mut self, value: &str) -> Rc<str> {
        let value = Rc::<str>::from(value);
        self.strings.push(Rc::downgrade(&value));
        value
    }

    pub(crate) fn alloc_function(&mut self, function: Function) -> Rc<Function> {
        let function = Rc::new(function);
        self.functions.push(Rc::downgrade(&function));
        function
    }

    pub(crate) fn alloc_array(&mut self, array: Array) -> Rc<Array> {
        let array = Rc::new(array);
        self.arrays.push(Rc::downgrade(&array));
        array
    }

    pub(crate) fn alloc_tuple(&mut self, tuple: Tuple) -> Rc<Tuple> {
        let tuple = Rc::new(tuple);
        self.tuples.push(Rc::downgrade(&tuple));
        tuple
    }

    pub(crate) fn alloc_map(&mut self, map: Map) -> Rc<Map> {
        let map = Rc::new(map);
        self.maps.push(Rc::downgrade(&map));
        map
    }

    pub(crate) fn alloc_option(&mut self, option: OptionValue) -> Rc<OptionValue> {
        let option = Rc::new(option);
        self.options.push(Rc::downgrade(&option));
        option
    }

    pub(crate) fn alloc_result(&mut self, result: ResultValue) -> Rc<ResultValue> {
        let result = Rc::new(result);
        self.results.push(Rc::downgrade(&result));
        result
    }

    pub(crate) fn alloc_enum(&mut self, value: EnumValue) -> Rc<EnumValue> {
        let value = Rc::new(value);
        self.enums.push(Rc::downgrade(&value));
        value
    }

    pub(crate) fn alloc_record(&mut self, record: Record) -> Rc<Record> {
        let record = Rc::new(record);
        self.records.push(Rc::downgrade(&record));
        record
    }

    pub(crate) fn track_value(&mut self, value: &Value) {
        self.collect();
        self.track_value_inner(value);
    }

    fn track_value_inner(&mut self, value: &Value) {
        match value {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {}
            Value::String(value) => track_rc(&mut self.strings, value),
            Value::Json(value) => track_rc(&mut self.jsons, value),
            Value::Array(array) => {
                track_rc(&mut self.arrays, array);
                for element in array.elements() {
                    self.track_value_inner(&element);
                }
            }
            Value::Tuple(tuple) => {
                track_rc(&mut self.tuples, tuple);
                for element in tuple.elements() {
                    self.track_value_inner(element);
                }
            }
            Value::Map(map) => {
                track_rc(&mut self.maps, map);
                for value in map.entries().values() {
                    self.track_value_inner(value);
                }
            }
            Value::Option(option) => {
                track_rc(&mut self.options, option);
                if let Some(payload) = option.payload() {
                    self.track_value_inner(payload);
                }
            }
            Value::Result(result) => {
                track_rc(&mut self.results, result);
                self.track_value_inner(result.payload());
            }
            Value::Enum(value) => {
                track_rc(&mut self.enums, value);
                if let Some(payload) = value.payload() {
                    self.track_value_inner(payload);
                }
            }
            Value::Record(record) => {
                track_rc(&mut self.records, record);
                for value in record.fields().values() {
                    self.track_value_inner(value);
                }
            }
            Value::Function(function) => track_rc(&mut self.functions, function),
        }
    }

    pub(crate) fn collect(&mut self) {
        self.strings.retain(|value| value.strong_count() > 0);
        self.jsons.retain(|value| value.strong_count() > 0);
        self.arrays.retain(|array| array.strong_count() > 0);
        self.tuples.retain(|tuple| tuple.strong_count() > 0);
        self.maps.retain(|map| map.strong_count() > 0);
        self.options.retain(|option| option.strong_count() > 0);
        self.results.retain(|result| result.strong_count() > 0);
        self.enums.retain(|value| value.strong_count() > 0);
        self.records.retain(|record| record.strong_count() > 0);
        self.functions
            .retain(|function| function.strong_count() > 0);
    }

    pub(crate) fn object_count(&self) -> usize {
        self.strings
            .iter()
            .filter(|value| value.strong_count() > 0)
            .count()
            + self
                .jsons
                .iter()
                .filter(|value| value.strong_count() > 0)
                .count()
            + self
                .arrays
                .iter()
                .filter(|array| array.strong_count() > 0)
                .count()
            + self
                .tuples
                .iter()
                .filter(|tuple| tuple.strong_count() > 0)
                .count()
            + self
                .maps
                .iter()
                .filter(|map| map.strong_count() > 0)
                .count()
            + self
                .options
                .iter()
                .filter(|option| option.strong_count() > 0)
                .count()
            + self
                .results
                .iter()
                .filter(|result| result.strong_count() > 0)
                .count()
            + self
                .enums
                .iter()
                .filter(|value| value.strong_count() > 0)
                .count()
            + self
                .records
                .iter()
                .filter(|record| record.strong_count() > 0)
                .count()
            + self
                .functions
                .iter()
                .filter(|function| function.strong_count() > 0)
                .count()
    }
}

fn track_rc<T: ?Sized>(items: &mut Vec<Weak<T>>, value: &Rc<T>) {
    if items
        .iter()
        .filter_map(Weak::upgrade)
        .any(|existing| Rc::ptr_eq(&existing, value))
    {
        return;
    }
    items.push(Rc::downgrade(value));
}
