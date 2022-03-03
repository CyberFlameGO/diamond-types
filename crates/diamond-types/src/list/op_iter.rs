use smallvec::SmallVec;
use rle::HasLength;
use crate::list;
use crate::list::internal_op::OperationInternal;
use crate::list::{OpLog, switch, Time};
use crate::list::operation::Operation;
use crate::localtime::TimeSpan;
use crate::rle::{KVPair, RleVec};

#[derive(Debug)]
pub(crate) struct OpMetricsIter<'a> {
    list: &'a RleVec<KVPair<OperationInternal>>,
    ins_content: &'a str,
    del_content: &'a str,

    idx: usize,
    range: TimeSpan,
}

#[derive(Debug)]
pub(crate) struct OpIterFast<'a>(OpMetricsIter<'a>);

impl<'a> Iterator for OpMetricsIter<'a> {
    type Item = KVPair<OperationInternal>;

    fn next(&mut self) -> Option<Self::Item> {
        // I bet there's a more efficient way to write this function.
        if self.idx >= self.list.0.len() { return None; }

        let KVPair(mut time, mut c) = self.list[self.idx].clone();
        if time >= self.range.end { return None; }

        let content = list::switch(c.tag, self.ins_content, self.del_content);
        if time + c.len() > self.range.end {
            c.truncate(self.range.end - time, content);
        }

        if time < self.range.start {
            c.truncate_keeping_right(self.range.start - time, content);
            time = self.range.start;
        }

        self.idx += 1;
        Some(KVPair(time, c))
    }
}

impl<'a> Iterator for OpIterFast<'a> {
    type Item = (KVPair<OperationInternal>, Option<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        let metrics = self.0.next()?;
        let content = self.0.get_content(&metrics);
        Some((metrics, content))
    }
}

impl<'a> OpMetricsIter<'a> {
    fn new(list: &'a RleVec<KVPair<OperationInternal>>, ins_content: &'a str, del_content: &'a str, range: TimeSpan) -> Self {
        let mut iter = OpMetricsIter {
            list,
            ins_content,
            del_content,
            idx: 0,
            range
        };
        iter.prime(range);
        iter
    }

    fn prime(&mut self, range: TimeSpan) {
        self.range = range;
        self.idx = if range.is_empty() { 0 } else { self.list.find_index(range.start).unwrap() };
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.idx >= self.list.0.len() || self.range.is_empty()
    }

    pub(crate) fn get_content(&self, metrics: &KVPair<OperationInternal>) -> Option<&'a str> {
        metrics.1.content_pos.map(|pos| {
            let c = switch(metrics.1.tag, self.ins_content, self.del_content);
            &c[pos.start..pos.end]
        })
    }
}

impl<'a> OpIterFast<'a> {
    fn new(oplog: &'a OpLog, range: TimeSpan) -> Self {
        Self(OpMetricsIter::new(&oplog.operations, &oplog.ins_content, &oplog.del_content, range))
    }
}

/// This is a variant on OpIterFast which returns operations since some (complex) point in time in
/// a document.
#[derive(Debug)]
struct OpIterRanges<'a> {
    ranges: SmallVec<[TimeSpan; 4]>, // We own this. This is in descending order.
    current: OpIterFast<'a>
}

impl<'a> OpIterRanges<'a> {
    fn new(oplog: &'a OpLog, mut r: SmallVec<[TimeSpan; 4]>) -> Self {
        let last = r.pop().unwrap_or((0..0).into());
        Self {
            ranges: r,
            current: OpIterFast::new(oplog, last)
        }
    }
}

impl<'a> Iterator for OpIterRanges<'a> {
    // type Item = KVPair<OperationInternal>;
    type Item = (KVPair<OperationInternal>, Option<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        let inner_next = self.current.next();
        if inner_next.is_some() { return inner_next; }

        if let Some(range) = self.ranges.pop() {
            debug_assert!(!range.is_empty());
            self.current.0.prime(range);
            let inner_next = self.current.next();
            if inner_next.is_some() { return inner_next; }
        }

        None
    }
}

impl OpLog {
    // TODO: Consider removing these functions if they're never used.
    #[allow(unused)]
    pub(crate) fn iter_metrics_range(&self, range: TimeSpan) -> OpMetricsIter {
        OpMetricsIter::new(&self.operations, &self.ins_content, &self.del_content, range)
    }

    #[allow(unused)]
    pub(crate) fn iter_metrics(&self) -> OpMetricsIter {
        self.iter_metrics_range((0..self.len()).into())
    }

    pub(crate) fn iter_range_simple(&self, range: TimeSpan) -> OpIterFast {
        OpIterFast::new(self, range)
    }

    pub fn iter_range_since(&self, frontier: &[Time]) -> impl Iterator<Item=Operation> + '_ {
        let (only_a, only_b) = self.history.diff(frontier, &self.frontier);
        assert!(only_a.is_empty());

        OpIterRanges::new(self, only_b)
            .map(|pair| (pair.0.1, pair.1).into())
    }

    pub(crate) fn iter_fast(&self) -> OpIterFast {
        OpIterFast::new(self, (0..self.len()).into())
    }

    pub fn iter(&self) -> impl Iterator<Item = Operation> + '_ {
        self.iter_fast().map(|pair| (pair.0.1, pair.1).into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::list::operation::InsDelTag;
    use crate::rle::{KVPair, RleVec};
    use InsDelTag::*;

    #[test]
    fn iter_smoke() {
        let mut ops: RleVec<KVPair<OperationInternal>> = RleVec::new();

        ops.push(KVPair(0, OperationInternal {
            span: (100..110).into(),
            tag: Ins,
            content_pos: Some((0..10).into()),
        }));
        ops.push(KVPair(10, OperationInternal {
            span: (200..220).into(),
            tag: Del,
            content_pos: None,
        }));

        let ins_content = "0123456789";
        let del_content = "";

        assert_eq!(OpMetricsIter::new(&ops, ins_content, del_content, (0..30).into()).collect::<Vec<_>>(), ops.0.as_slice());
        
        assert_eq!(OpMetricsIter::new(&ops, ins_content, del_content, (1..5).into()).collect::<Vec<_>>(), &[KVPair(1, OperationInternal {
            span: (101..105).into(),
            tag: Ins,
            content_pos: Some((1..5).into()),
        })]);

        assert_eq!(OpMetricsIter::new(&ops, ins_content, del_content, (6..16).into()).collect::<Vec<_>>(), &[
            KVPair(6, OperationInternal {
                span: (106..110).into(),
                tag: Ins,
                content_pos: Some((6..10).into()),
            }),
            KVPair(10, OperationInternal {
                span: (200..206).into(),
                tag: Del,
                content_pos: None,
            }),
        ]);
    }
}