use chrono::NaiveDateTime;
use eyre::Result;

use crate::storage::{BoxState, Data, Date, Task};

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub struct TaskID(usize);

#[derive(Debug)]
pub struct FilteredData {
    data: Data,
    visible: Vec<usize>,
    filter: Option<BooleanExpr>,
}
impl FilteredData {
    pub fn new(data: Data) -> Self {
        Self {
            visible: (0..data.tasks().len()).collect(),
            data,
            filter: None,
        }
    }
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            data: &self.data,
            iter: self.visible.iter(),
        }
    }
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    pub fn get_id(&self, visible_index: usize) -> TaskID {
        TaskID(self.visible[visible_index])
    }
    pub fn get(&self, task_id: TaskID) -> Option<&Task> {
        Some(&self.data.tasks()[task_id.0])
    }
    pub fn get_mut(&mut self, task_id: TaskID) -> Option<&mut Task> {
        Some(&mut self.data.tasks_mut()[task_id.0])
    }
    pub fn set_completed(&mut self, index: usize, value: Option<Date>) {
        self.data.set_completed(self.visible[index], value);
        self.recalculate_is_visible(index);
    }
    pub fn push_box(&mut self, index: usize) {
        self.data.push_box(self.visible[index]);
        self.recalculate_is_visible(index);
    }
    pub fn step_box_state(&mut self, index: usize, time: Date) -> Option<BoxState> {
        let step_box_state = self.data.step_box_state(self.visible[index], time);
        self.recalculate_is_visible(index);
        step_box_state
    }
    pub fn remove_empty_state(&mut self, index: usize) {
        self.data.remove_empty_state(self.visible[index]);
        self.recalculate_is_visible(index);
    }

    pub fn write_dirty(&mut self) -> Result<()> {
        self.data.write_dirty()
    }
    pub fn push(&mut self, task: Task) {
        let new_index = self.data.tasks().len();
        let visible_index = self.visible.len();
        self.visible.push(new_index);
        self.data.push(task);
        self.recalculate_is_visible(visible_index);
    }
    fn recalculate_is_visible(&mut self, visible_index: usize) {
        let Some(expr) = &self.filter else {
            return;
        };
        if !self.data.tasks()[self.visible[visible_index]].satisfies(expr) {
            self.visible.remove(visible_index);
        }
    }

    pub fn set_filter(&mut self, input: &str) -> Result<()> {
        let expr = BooleanExpr::from_str(input)?;
        self.visible = self
            .data
            .tasks()
            .iter()
            .enumerate()
            .filter(|(_, t)| expr.as_ref().is_none_or(|expr| t.satisfies(expr)))
            .map(|(i, _)| i)
            .collect();
        self.filter = expr;
        Ok(())
    }
}

pub struct Iter<'a> {
    data: &'a Data,
    iter: std::slice::Iter<'a, usize>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Task;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|i| &self.data.tasks()[*i])
    }
}
impl<'a> ExactSizeIterator for Iter<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl Task {
    fn get_box(&self, index: isize) -> Option<BoxState> {
        if index >= 0 {
            self.boxes().get(index as usize).copied()
        } else {
            // -1 should be last
            // -len should be 0th
            let offset = (-index) as usize;
            if offset <= self.boxes().len() {
                self.boxes().get(self.boxes().len() - offset).copied()
            } else {
                None
            }
        }
    }

    fn satisfies(&self, expr: &BooleanExpr) -> bool {
        match expr {
            BooleanExpr::Not(boolean_expr) => !self.satisfies(boolean_expr),
            BooleanExpr::Compound { combinator, exprs } => {
                let mut it = exprs.iter().map(|e| self.satisfies(e));
                match combinator {
                    Comb::And => it.all(|e| e),
                    Comb::Or => it.any(|e| e),
                }
            }
            BooleanExpr::Comparison {
                comparator,
                lhs,
                rhs,
            } => {
                let (lhs, rhs) = (self.eval(lhs), self.eval(rhs));
                match comparator {
                    Comp::Leq => lhs <= rhs,
                    Comp::Geq => lhs >= rhs,
                    Comp::Eq => lhs == rhs,
                }
            }
            BooleanExpr::Tag(t) => self.tags().contains(t),
            BooleanExpr::Box { index } => self.get_box(*index).is_some(),
            BooleanExpr::Completed => self.completed().is_some(),
            BooleanExpr::Const(b) => *b,
        }
    }
    fn eval(&self, expr: &ValueExpr) -> Value {
        match expr {
            ValueExpr::Date(naive_date) => Value::Date(Some(*naive_date)),
            ValueExpr::Box { index } => Value::Box(self.get_box(*index)),
            ValueExpr::Completed => Value::Date(*self.completed()),
            ValueExpr::Created => Value::Date(Some(*self.created())),
            ValueExpr::Started => Value::Box(Some(BoxState::Started)),
            ValueExpr::Empty => Value::Box(Some(BoxState::Empty)),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BooleanExpr {
    Not(Box<BooleanExpr>),
    Compound {
        combinator: Comb,
        exprs: Vec<BooleanExpr>,
    },
    Comparison {
        comparator: Comp,
        lhs: ValueExpr,
        rhs: ValueExpr,
    },
    Tag(String),
    Box {
        index: isize,
    },
    Completed,
    Const(bool),
}

#[derive(Clone, Debug)]
pub enum ValueExpr {
    Date(NaiveDateTime),
    Box { index: isize },
    Completed,
    Created,
    Started,
    Empty,
}

enum Value {
    Date(Option<NaiveDateTime>),
    Box(Option<BoxState>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Date(l), Self::Date(r)) => l.partial_cmp(r),
            (Self::Box(l), Self::Box(r)) => match (l.as_ref()?, r.as_ref()?) {
                (BoxState::Checked(l), BoxState::Checked(r)) => l.partial_cmp(r),
                (BoxState::Started, BoxState::Started) => Some(std::cmp::Ordering::Equal),
                (BoxState::Empty, BoxState::Empty) => Some(std::cmp::Ordering::Equal),
                _ => None,
            },
            (Self::Date(l), Self::Box(r)) | (Self::Box(r), Self::Date(l)) => {
                match (l.as_ref()?, r.as_ref()?) {
                    (date, BoxState::Checked(box_time)) => date.partial_cmp(box_time),
                    _ => None,
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Comb {
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Comp {
    Leq,
    Geq,
    Eq,
}

mod parser {
    // filter expression grammar:
    // filter = '(' delimited(filter, '|') ')' | '(' delimited(filter, '&') ')' | 'not' filter | existence | comparison
    // existence = 'completed' | 'box'[i]
    // comparison = value operator reference
    // value = 'created' | 'completed' | 'box'[i] | 'started' | 'empty'
    // operator = '>=' | '<=' | '='
    // reference = date
    // date = '"' \d{4}-\d{2}-\d{2} \d{2}:\d{2} '"'
    //
    // maybe in future also, 'name' 'contains' string
    //

    use chrono::NaiveDate;
    use chumsky::{
        Parser,
        error::Rich,
        extra,
        prelude::*,
        text::{digits, whitespace},
    };
    use eyre::{Result, eyre};

    use crate::filter::{BooleanExpr, Comp, ValueExpr};

    impl super::BooleanExpr {
        pub fn from_str(input: &str) -> Result<Option<super::BooleanExpr>> {
            expr()
                .map(Some)
                .or(empty().to(None))
                .parse(input)
                .into_result()
                .map_err(|e| {
                    let Some(e) = e.first() else {
                        return eyre!("missing error");
                    };
                    eyre!(
                        "parsing fields encountered {} at char {}",
                        e.reason(),
                        e.span().start
                    )
                })
        }
    }

    fn expr<'src>() -> impl Parser<'src, &'src str, BooleanExpr, extra::Err<Rich<'src, char>>> {
        fn parse_int<'src>(
            n: &'src str,
            span: SimpleSpan,
        ) -> std::result::Result<isize, Rich<'src, char>> {
            n.parse::<isize>().map_err(|e| Rich::custom(span, e))
        }
        fn parse_uint<'src>(
            n: &'src str,
            span: SimpleSpan,
        ) -> std::result::Result<usize, Rich<'src, char>> {
            n.parse::<usize>().map_err(|e| Rich::custom(span, e))
        }
        fn digit_count<'src>(
            count: usize,
        ) -> impl Parser<'src, &'src str, usize, extra::Err<Rich<'src, char>>> + Clone {
            digits(10).exactly(count).to_slice().try_map(parse_uint)
        }
        let date_expr = choice((
            just("completed").to(ValueExpr::Completed),
            just("created").to(ValueExpr::Created),
            just("box[").ignore_then(
                just("-")
                    .to(())
                    .or(empty())
                    .then(digits(10).repeated())
                    .to_slice()
                    .try_map(parse_int)
                    .then_ignore(just("]"))
                    .map(|index| ValueExpr::Box { index }),
            ),
            digit_count(4)
                .then_ignore(just("-"))
                .then(digit_count(2))
                .then_ignore(just("-"))
                .then(digit_count(2))
                .then_ignore(whitespace())
                .then(digit_count(2))
                .then_ignore(just(":"))
                .then(digit_count(2))
                .try_map(|((((y, m), d), hour), min), span| {
                    NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32)
                        .and_then(|d| d.and_hms_opt(hour as u32, min as u32, 0))
                        .ok_or_else(|| Rich::custom(span, "invalid date"))
                })
                .map(ValueExpr::Date),
            just("started").to(ValueExpr::Started),
            just("empty").to(ValueExpr::Empty),
        ))
        .padded();

        recursive(|expr| {
            choice((
                just("not ")
                    .ignore_then(expr.clone())
                    .map(|e| BooleanExpr::Not(Box::new(e))),
                expr.clone()
                    .separated_by(just('|'))
                    .collect::<Vec<_>>()
                    .delimited_by(just('('), just(')'))
                    .map(|exprs| BooleanExpr::Compound {
                        combinator: super::Comb::Or,
                        exprs,
                    }),
                expr.clone()
                    .separated_by(just('&'))
                    .collect::<Vec<_>>()
                    .delimited_by(just('('), just(')'))
                    .map(|exprs| BooleanExpr::Compound {
                        combinator: super::Comb::And,
                        exprs,
                    }),
                date_expr
                    .clone()
                    .then(choice((
                        just("<=").to(Comp::Leq),
                        just(">=").to(Comp::Geq),
                        just("=").to(Comp::Eq),
                    )))
                    .then(date_expr.clone())
                    .map(|((lhs, comparator), rhs)| BooleanExpr::Comparison {
                        lhs,
                        rhs,
                        comparator,
                    }),
                just("tag(")
                    .ignore_then(
                        any()
                            .filter(|c: &char| *c != '(' && *c != ')')
                            .repeated()
                            .at_least(1)
                            .collect::<String>(),
                    )
                    .then_ignore(just(")"))
                    .map(BooleanExpr::Tag),
                just("box[").ignore_then(
                    just("-")
                        .to(())
                        .or(empty())
                        .then(digits(10).repeated())
                        .to_slice()
                        .try_map(parse_int)
                        .then_ignore(just("]"))
                        .map(|index| BooleanExpr::Box { index }),
                ),
                just("completed").to(BooleanExpr::Completed),
                just("true")
                    .to(true)
                    .or(just("false").to(false))
                    .map(BooleanExpr::Const),
            ))
            .padded()
        })
    }
}
