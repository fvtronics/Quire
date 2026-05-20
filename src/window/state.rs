use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct MergeState {
    pub files: RefCell<Vec<PathBuf>>,
    pub rotations: RefCell<BTreeMap<PathBuf, i64>>,
    pub previews: RefCell<BTreeMap<PathBuf, crate::preview::PagePreview>>,
    pub last_output: RefCell<Option<PathBuf>>,
    pub is_loading: Cell<bool>,
}

#[derive(Debug, Default)]
pub struct CompressState {
    pub file: RefCell<Option<PathBuf>>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub last_output: RefCell<Option<PathBuf>>,
    pub is_loading: Cell<bool>,
}

#[derive(Debug, Default)]
pub struct OrganizeState {
    pub file: RefCell<Option<PathBuf>>,
    pub page_count: Cell<usize>,
    pub previews: RefCell<Vec<crate::preview::PagePreview>>,
    pub page_order: RefCell<Vec<u32>>,
    pub rotations: RefCell<BTreeMap<u32, i64>>,
    pub last_output: RefCell<Option<PathBuf>>,
}

#[derive(Debug, Default)]
pub struct ExtractState {
    pub file: RefCell<Option<PathBuf>>,
    pub page_count: Cell<usize>,
    pub previews: RefCell<Vec<crate::preview::PagePreview>>,
    pub selected_pages: RefCell<Vec<u32>>,
    pub rotations: RefCell<BTreeMap<u32, i64>>,
    pub last_output: RefCell<Option<PathBuf>>,
}

#[derive(Debug, Default)]
pub struct SplitState {
    pub file: RefCell<Option<PathBuf>>,
    pub page_count: Cell<usize>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub last_output: RefCell<Option<PathBuf>>,
    pub is_loading: Cell<bool>,
}

impl MergeState {
    pub fn clear(&self) {
        self.files.borrow_mut().clear();
        self.rotations.borrow_mut().clear();
        self.previews.borrow_mut().clear();
        self.last_output.borrow_mut().take();
    }

    pub fn paths_needing_previews(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let previews = self.previews.borrow();
        paths
            .iter()
            .filter(|path| !previews.contains_key(*path))
            .cloned()
            .collect()
    }

    pub fn add_files(&self, paths: Vec<PathBuf>) {
        self.files.borrow_mut().extend(paths);
        self.last_output.borrow_mut().take();
    }

    pub fn begin_loading(&self) {
        self.is_loading.set(true);
        self.last_output.borrow_mut().take();
    }

    pub fn finish_loading(
        &self,
        paths: Vec<PathBuf>,
        previews: Vec<(PathBuf, crate::preview::PagePreview)>,
    ) {
        self.is_loading.set(false);
        self.previews.borrow_mut().extend(previews);
        self.files.borrow_mut().extend(paths);
    }

    pub fn pdf_inputs(&self) -> Vec<crate::pdf::PdfInput> {
        let rotations = self.rotations.borrow();
        self.files
            .borrow()
            .iter()
            .map(|path| crate::pdf::PdfInput {
                path: path.clone(),
                rotation: *rotations.get(path).unwrap_or(&0),
            })
            .collect()
    }

    pub fn clear_last_output(&self) {
        self.last_output.borrow_mut().take();
    }

    pub fn set_last_output(&self, path: PathBuf) {
        self.last_output.borrow_mut().replace(path);
    }

    pub fn is_busy(&self, is_running: bool) -> bool {
        is_running || self.is_loading.get()
    }

    pub fn move_file(&self, from: usize, to: usize) {
        self.files.borrow_mut().swap(from, to);
        self.clear_last_output();
    }

    pub fn rotate_file(&self, index: usize) -> bool {
        let Some(path) = self.files.borrow().get(index).cloned() else {
            return false;
        };
        rotate_entry(&self.rotations, path);
        self.clear_last_output();
        true
    }

    pub fn reorder_file(&self, from: usize, to: usize) -> bool {
        if from == to {
            return false;
        }

        let mut files = self.files.borrow_mut();
        if from >= files.len() || to >= files.len() {
            return false;
        }

        let file = files.remove(from);
        files.insert(to, file);
        drop(files);
        self.clear_last_output();
        true
    }

    pub fn remove_file(&self, index: usize) {
        let path = self.files.borrow_mut().remove(index);
        if !self.files.borrow().contains(&path) {
            self.rotations.borrow_mut().remove(&path);
        }
        self.clear_last_output();
    }
}

impl CompressState {
    pub fn input_file(&self) -> Option<PathBuf> {
        self.file.borrow().clone()
    }

    pub fn begin_loading(&self) {
        self.is_loading.set(true);
        self.clear_last_output();
    }

    pub fn finish_loading(&self, path: PathBuf, preview: Option<crate::preview::PagePreview>) {
        self.is_loading.set(false);
        self.file.borrow_mut().replace(path);
        *self.preview.borrow_mut() = preview;
    }

    pub fn finish_loading_failed(&self) {
        self.is_loading.set(false);
    }

    pub fn clear_last_output(&self) {
        self.last_output.borrow_mut().take();
    }

    pub fn set_last_output(&self, path: PathBuf) {
        self.last_output.borrow_mut().replace(path);
    }

    pub fn is_busy(&self, is_running: bool) -> bool {
        is_running || self.is_loading.get()
    }
}

impl OrganizeState {
    pub fn load_document(&self, path: PathBuf, previews: Vec<crate::preview::PagePreview>) {
        let page_count = previews.len();
        self.file.borrow_mut().replace(path);
        self.page_count.set(page_count);
        *self.previews.borrow_mut() = previews;

        let mut page_order = self.page_order.borrow_mut();
        page_order.clear();
        page_order.extend(1..=page_count as u32);

        self.rotations.borrow_mut().clear();
        self.clear_last_output();
    }

    pub fn reset(&self) -> bool {
        let page_count = self.page_count.get();
        if self.file.borrow().is_none() || page_count == 0 {
            return false;
        }

        let mut page_order = self.page_order.borrow_mut();
        page_order.clear();
        page_order.extend(1..=page_count as u32);
        self.rotations.borrow_mut().clear();
        self.clear_last_output();
        true
    }

    pub fn selections(&self) -> Option<(PathBuf, Vec<crate::pdf::PageSelection>)> {
        let input_file = self.file.borrow().clone()?;
        let rotations = self.rotations.borrow();
        let pages = self
            .page_order
            .borrow()
            .iter()
            .map(|page_number| crate::pdf::PageSelection {
                page_number: *page_number,
                rotation: *rotations.get(page_number).unwrap_or(&0),
            })
            .collect();

        Some((input_file, pages))
    }

    pub fn clear_last_output(&self) {
        self.last_output.borrow_mut().take();
    }

    pub fn set_last_output(&self, path: PathBuf) {
        self.last_output.borrow_mut().replace(path);
    }

    pub fn move_page(&self, from: usize, to: usize) {
        self.page_order.borrow_mut().swap(from, to);
        self.clear_last_output();
    }

    pub fn rotate_page(&self, page_number: u32) {
        rotate_entry(&self.rotations, page_number);
        self.clear_last_output();
    }

    pub fn reorder_page(&self, dragged_page: u32, target_page: u32) -> bool {
        if dragged_page == target_page {
            return false;
        }

        let mut pages = self.page_order.borrow_mut();
        let Some(from) = pages.iter().position(|page| *page == dragged_page) else {
            return false;
        };
        let Some(to) = pages.iter().position(|page| *page == target_page) else {
            return false;
        };

        let page = pages.remove(from);
        pages.insert(to, page);
        drop(pages);
        self.clear_last_output();
        true
    }

    pub fn remove_page(&self, index: usize) -> bool {
        if self.page_order.borrow().len() <= 1 {
            return false;
        }

        let page_number = self.page_order.borrow_mut().remove(index);
        self.rotations.borrow_mut().remove(&page_number);
        self.clear_last_output();
        true
    }
}

impl ExtractState {
    pub fn clear_range_selection(&self) {
        self.selected_pages.borrow_mut().clear();
        self.rotations.borrow_mut().clear();
    }

    pub fn apply_range_selection(&self, pages: Vec<u32>) {
        self.rotations
            .borrow_mut()
            .retain(|page_number, _| pages.contains(page_number));
        *self.selected_pages.borrow_mut() = pages;
    }

    pub fn load_document(&self, path: PathBuf, previews: Vec<crate::preview::PagePreview>) {
        let page_count = previews.len();
        self.file.borrow_mut().replace(path);
        self.page_count.set(page_count);
        *self.previews.borrow_mut() = previews;
        self.clear_range_selection();
        self.clear_last_output();
    }

    pub fn selections_from_pages(
        &self,
        page_numbers: Vec<u32>,
    ) -> Option<(PathBuf, Vec<crate::pdf::PageSelection>)> {
        let input_file = self.file.borrow().clone()?;
        let rotations = self.rotations.borrow();
        let pages = page_numbers
            .into_iter()
            .map(|page_number| crate::pdf::PageSelection {
                page_number,
                rotation: *rotations.get(&page_number).unwrap_or(&0),
            })
            .collect();

        Some((input_file, pages))
    }

    pub fn clear_last_output(&self) {
        self.last_output.borrow_mut().take();
    }

    pub fn set_last_output(&self, path: PathBuf) {
        self.last_output.borrow_mut().replace(path);
    }

    pub fn toggle_page(&self, page_number: u32, selected: bool) {
        let mut pages = self.selected_pages.borrow_mut();

        if selected {
            if !pages.contains(&page_number) {
                pages.push(page_number);
                pages.sort_unstable();
            }
        } else {
            pages.retain(|page| *page != page_number);
            self.rotations.borrow_mut().remove(&page_number);
        }

        drop(pages);
        self.clear_last_output();
    }

    pub fn rotate_page(&self, page_number: u32) -> bool {
        if !self.selected_pages.borrow().contains(&page_number) {
            return false;
        }

        rotate_entry(&self.rotations, page_number);
        self.clear_last_output();
        true
    }
}

impl SplitState {
    pub fn input_file(&self) -> Option<PathBuf> {
        self.file.borrow().clone()
    }

    pub fn begin_loading(&self) {
        self.is_loading.set(true);
        self.clear_last_output();
    }

    pub fn finish_loading(
        &self,
        path: PathBuf,
        preview: Option<crate::preview::PagePreview>,
        page_count: usize,
    ) {
        self.is_loading.set(false);
        self.file.borrow_mut().replace(path);
        self.page_count.set(page_count);
        *self.preview.borrow_mut() = preview;
    }

    pub fn finish_loading_failed(&self) {
        self.is_loading.set(false);
    }

    pub fn clear_last_output(&self) {
        self.last_output.borrow_mut().take();
    }

    pub fn set_last_output(&self, path: PathBuf) {
        self.last_output.borrow_mut().replace(path);
    }

    pub fn is_busy(&self, is_running: bool) -> bool {
        is_running || self.is_loading.get()
    }
}

fn rotate_entry<Key>(rotations: &RefCell<BTreeMap<Key, i64>>, key: Key)
where
    Key: Ord,
{
    let mut rotations = rotations.borrow_mut();
    let rotation = (rotations.get(&key).copied().unwrap_or(0) + 90).rem_euclid(360);
    if rotation == 0 {
        rotations.remove(&key);
    } else {
        rotations.insert(key, rotation);
    }
}
