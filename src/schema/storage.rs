use bumpalo::Bump;
use bytemuck::Pod;

use crate::schema::formats::View;

pub struct FormatStorage {
    alloc: Bump,
}

impl PartialEq for FormatStorage {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl FormatStorage {
    pub fn new() -> Self {
        Self { alloc: Bump::new() }
    }

    pub(crate) fn alloc<T, E>(&self, len: usize, fill: impl FnMut(usize) -> Result<T, E>) -> Result<&[T], E> {
        self.alloc.alloc_slice_try_fill_with(len, fill).map(|val| &*val)
    }

    pub(crate) fn make_view<T: Pod>(&self, data: &[T]) -> View<'_, T> {
        let data = self.alloc::<T, ()>(data.len(), |index| Ok(data[index])).unwrap();
        View::new(data)
    }
}
