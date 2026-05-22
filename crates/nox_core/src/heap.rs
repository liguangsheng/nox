use std::rc::{Rc, Weak};

use crate::{Array, Function, Map, OptionValue, Record, ResultValue};

#[derive(Debug, Default)]
pub(crate) struct GcHeap {
    strings: Vec<Weak<str>>,
    arrays: Vec<Weak<Array>>,
    maps: Vec<Weak<Map>>,
    options: Vec<Weak<OptionValue>>,
    results: Vec<Weak<ResultValue>>,
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

    pub(crate) fn alloc_record(&mut self, record: Record) -> Rc<Record> {
        let record = Rc::new(record);
        self.records.push(Rc::downgrade(&record));
        record
    }

    pub(crate) fn collect(&mut self) {
        self.strings.retain(|value| value.strong_count() > 0);
        self.arrays.retain(|array| array.strong_count() > 0);
        self.maps.retain(|map| map.strong_count() > 0);
        self.options.retain(|option| option.strong_count() > 0);
        self.results.retain(|result| result.strong_count() > 0);
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
                .arrays
                .iter()
                .filter(|array| array.strong_count() > 0)
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
