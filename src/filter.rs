use chrono::NaiveDateTime;
use eyre::Result;

use crate::storage::{BoxState, Data, Date, Task};

#[derive(PartialEq, Clone, Copy)]
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
    // TODO: only pass out a reference to the inner text of the task
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Task> {
        let i = self.visible.get(index)?;
        Some(&mut self.data.tasks_mut()[*i])
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
        // TODO: properly recalculate visible
        let new_index = self.data.tasks().len();
        self.visible.push(new_index);
        self.data.push(task);
        self.recalculate_is_visible(new_index);
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
                let (Some(lhs), Some(rhs)) = (self.eval(lhs), self.eval(rhs)) else {
                    return false;
                };
                match comparator {
                    Comp::Leq => lhs <= rhs,
                    Comp::Geq => lhs >= rhs,
                    Comp::Eq => lhs == rhs,
                }
            }
            BooleanExpr::Tag(t) => self.tags().contains(t),
            BooleanExpr::Box { index } => self.boxes().get(*index).is_some(),
            BooleanExpr::Completed => self.completed().is_some(),
        }
    }
    fn eval(&self, expr: &DateExpr) -> Option<NaiveDateTime> {
        match expr {
            DateExpr::Date(naive_date) => Some(*naive_date),
            DateExpr::Box { index } => self.boxes().get(*index).and_then(|b| {
                if let BoxState::Checked(naive_date_time) = b {
                    Some(*naive_date_time)
                } else {
                    None
                }
            }),
            DateExpr::Completed => *self.completed(),
            DateExpr::Created => Some(*self.created()),
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
        lhs: DateExpr,
        rhs: DateExpr,
    },
    Tag(String),
    Box {
        index: usize,
    },
    Completed,
}

#[derive(Clone, Debug)]
pub enum DateExpr {
    Date(NaiveDateTime),
    Box { index: usize },
    Completed,
    Created,
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
    use chrono::NaiveDate;
    use chumsky::{
        Parser,
        error::Rich,
        extra,
        prelude::*,
        text::{digits, whitespace},
    };
    use eyre::{Result, eyre};

    use crate::filter::{BooleanExpr, Comp, DateExpr};

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
        ) -> std::result::Result<usize, Rich<'src, char>> {
            n.parse::<usize>().map_err(|e| Rich::custom(span, e))
        }
        fn digit_count<'src>(
            count: usize,
        ) -> impl Parser<'src, &'src str, usize, extra::Err<Rich<'src, char>>> + Clone {
            digits(10).exactly(count).to_slice().try_map(parse_int)
        }
        let date_expr = choice((
            just("completed").to(DateExpr::Completed),
            just("created").to(DateExpr::Created),
            just("box[").ignore_then(
                digits(10)
                    .repeated()
                    .to_slice()
                    .try_map(parse_int)
                    .then_ignore(just("]"))
                    .map(|index| DateExpr::Box { index }),
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
                .map(DateExpr::Date),
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
                    digits(10)
                        .repeated()
                        .to_slice()
                        .try_map(parse_int)
                        .then_ignore(just("]"))
                        .map(|index| BooleanExpr::Box { index }),
                ),
                just("completed").to(BooleanExpr::Completed),
            ))
            .padded()
        })
    }
}
