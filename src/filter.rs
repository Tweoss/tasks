use eyre::Result;

use crate::storage::{BoxState, Data, Date, Task};

#[derive(Debug)]
pub struct FilteredData {
    data: Data,
    visible: Vec<usize>,
}
impl FilteredData {
    pub fn new(data: Data) -> Self {
        Self {
            visible: (0..data.tasks().len()).collect(),
            data,
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

    // If we remove a task, visible should update.
    // Perhaps the index outside (e.g. the table index)
    // should also be updated => this function would
    // not return an option.
    pub fn get(&self, index: usize) -> Option<&Task> {
        let i = self.visible.get(index)?;
        Some(&self.data.tasks()[*i])
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Task> {
        let i = self.visible.get(index)?;
        Some(&mut self.data.tasks_mut()[*i])
    }
    pub fn set_completed(&mut self, index: usize, value: Option<Date>) {
        self.data.set_completed(self.visible[index], value);
    }
    pub fn push_box(&mut self, index: usize) {
        self.data.push_box(self.visible[index]);
    }
    pub fn step_box_state(&mut self, index: usize, time: Date) -> Option<BoxState> {
        self.data.step_box_state(self.visible[index], time)
    }
    pub fn remove_empty_state(&mut self, index: usize) {
        self.data.remove_empty_state(self.visible[index]);
    }

    pub fn write_dirty(&mut self) -> Result<()> {
        self.data.write_dirty()
    }
    pub fn push(&mut self, task: Task) {
        // TODO: properly recalculate visible
        self.visible.push(self.data.tasks().len());
        self.data.push(task);
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
