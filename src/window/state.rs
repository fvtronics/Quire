use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct JobState {
    last_output: RefCell<Option<PathBuf>>,
    is_loading: Cell<bool>,
}

impl JobState {
    pub fn begin_loading(&self) {
        self.is_loading.set(true);
        self.clear_last_output();
    }

    pub fn finish_loading(&self) {
        self.is_loading.set(false);
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

    pub fn last_output(&self) -> Option<PathBuf> {
        self.last_output.borrow().clone()
    }

    pub fn has_last_output(&self) -> bool {
        self.last_output.borrow().is_some()
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading.get()
    }

    pub fn is_busy(&self, is_running: bool) -> bool {
        is_running || self.is_loading()
    }
}

#[derive(Debug)]
pub struct SaveOptionsState {
    modern_pdf: Cell<bool>,
    remove_metadata: Cell<bool>,
}

impl Default for SaveOptionsState {
    fn default() -> Self {
        let options = crate::pdf::PdfSaveOptions::default();
        Self {
            modern_pdf: Cell::new(options.modern_pdf),
            remove_metadata: Cell::new(options.remove_metadata),
        }
    }
}

impl SaveOptionsState {
    pub fn modern_pdf(&self) -> bool {
        self.modern_pdf.get()
    }

    pub fn set_modern_pdf(&self, active: bool) {
        self.modern_pdf.set(active);
    }

    pub fn remove_metadata(&self) -> bool {
        self.remove_metadata.get()
    }

    pub fn set_remove_metadata(&self, active: bool) {
        self.remove_metadata.set(active);
    }

    pub fn options(&self) -> crate::pdf::PdfSaveOptions {
        crate::pdf::PdfSaveOptions {
            remove_metadata: self.remove_metadata.get(),
            modern_pdf: self.modern_pdf.get(),
        }
    }
}

#[derive(Debug)]
pub struct OutputOptionsState {
    save: SaveOptionsState,
    normalize_page_size: Cell<bool>,
}

impl Default for OutputOptionsState {
    fn default() -> Self {
        let options = crate::pdf::PdfOutputOptions::default();
        Self {
            save: SaveOptionsState::default(),
            normalize_page_size: Cell::new(options.normalize_page_size),
        }
    }
}

impl OutputOptionsState {
    pub fn save_state(&self) -> &SaveOptionsState {
        &self.save
    }

    pub fn modern_pdf(&self) -> bool {
        self.save.modern_pdf()
    }

    pub fn set_modern_pdf(&self, active: bool) {
        self.save.set_modern_pdf(active);
    }

    pub fn normalize_page_size(&self) -> bool {
        self.normalize_page_size.get()
    }

    pub fn set_normalize_page_size(&self, active: bool) {
        self.normalize_page_size.set(active);
    }

    pub fn remove_metadata(&self) -> bool {
        self.save.remove_metadata()
    }

    pub fn set_remove_metadata(&self, active: bool) {
        self.save.set_remove_metadata(active);
    }

    pub fn options(&self) -> crate::pdf::PdfOutputOptions {
        crate::pdf::PdfOutputOptions {
            normalize_page_size: self.normalize_page_size.get(),
            save: self.save.options(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MergeState {
    pub files: RefCell<Vec<MergeItem>>,
    pub passwords: RefCell<BTreeMap<PathBuf, String>>,
    pub previews: RefCell<BTreeMap<PathBuf, crate::preview::PagePreview>>,
    pub options: OutputOptionsState,
    pub job: JobState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeItem {
    pub path: PathBuf,
    pub rotation: i64,
}

#[derive(Debug)]
pub(super) struct MergeClearUndo {
    files: Vec<MergeItem>,
    passwords: BTreeMap<PathBuf, String>,
    previews: BTreeMap<PathBuf, crate::preview::PagePreview>,
}

#[derive(Debug)]
pub(super) struct MergeRemoveUndo {
    index: usize,
    item: MergeItem,
    password: Option<String>,
}

impl MergeItem {
    fn new(path: PathBuf) -> Self {
        Self { path, rotation: 0 }
    }

    fn rotate_clockwise(&mut self) {
        self.rotation = rotate_clockwise(self.rotation);
    }
}

#[derive(Debug, Default)]
pub struct CompressState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub options: SaveOptionsState,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct OrganizeState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub page_count: Cell<usize>,
    pub previews: RefCell<BTreeMap<u32, crate::preview::PagePreview>>,
    pub page_order: RefCell<Vec<crate::pdf::PageSelection>>,
    pub options: OutputOptionsState,
    pub job: JobState,
}

#[derive(Debug)]
pub(super) struct OrganizeResetUndo {
    page_order: Vec<crate::pdf::PageSelection>,
}

#[derive(Debug)]
pub(super) struct OrganizeRemoveUndo {
    index: usize,
    page: crate::pdf::PageSelection,
}

#[derive(Debug, Default)]
pub struct ExtractState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub page_count: Cell<usize>,
    pub previews: RefCell<BTreeMap<u32, crate::preview::PagePreview>>,
    pub selected_pages: RefCell<BTreeSet<u32>>,
    pub rotations: RefCell<BTreeMap<u32, i64>>,
    pub options: OutputOptionsState,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct SplitState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub page_count: Cell<usize>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub options: SaveOptionsState,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct MetadataState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub options: SaveOptionsState,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct WatermarkState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub page_count: Cell<usize>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub image_file: RefCell<Option<PathBuf>>,
    pub options: SaveOptionsState,
    pub job: JobState,
}

impl MergeState {
    pub(super) fn clear(&self) -> MergeClearUndo {
        let undo = MergeClearUndo {
            files: std::mem::take(&mut *self.files.borrow_mut()),
            passwords: std::mem::take(&mut *self.passwords.borrow_mut()),
            previews: std::mem::take(&mut *self.previews.borrow_mut()),
        };
        self.job.clear_last_output();
        undo
    }

    pub(super) fn restore_clear(&self, undo: MergeClearUndo) {
        *self.files.borrow_mut() = undo.files;
        *self.passwords.borrow_mut() = undo.passwords;
        *self.previews.borrow_mut() = undo.previews;
        self.job.clear_last_output();
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
        self.files
            .borrow_mut()
            .extend(paths.into_iter().map(MergeItem::new));
        self.job.clear_last_output();
    }

    pub fn finish_loading(
        &self,
        paths: Vec<PathBuf>,
        previews: Vec<(PathBuf, crate::preview::PagePreview)>,
        passwords: Vec<(PathBuf, Option<String>)>,
    ) {
        self.job.finish_loading();
        self.previews.borrow_mut().extend(previews);
        self.passwords.borrow_mut().extend(
            passwords
                .into_iter()
                .filter_map(|(path, password)| password.map(|password| (path, password))),
        );
        self.files
            .borrow_mut()
            .extend(paths.into_iter().map(MergeItem::new));
    }

    pub fn pdf_inputs(&self) -> Vec<crate::pdf::PdfInput> {
        let passwords = self.passwords.borrow();
        let files = self.files.borrow();
        files
            .iter()
            .map(|item| crate::pdf::PdfInput {
                path: item.path.clone(),
                password: passwords.get(&item.path).cloned(),
                rotation: item.rotation,
            })
            .collect()
    }

    pub fn move_file(&self, from: usize, to: usize) -> bool {
        let mut files = self.files.borrow_mut();
        if !move_vec_item(&mut files, from, to) {
            return false;
        }

        drop(files);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_file(&self, index: usize) -> bool {
        let mut files = self.files.borrow_mut();
        let Some(item) = files.get_mut(index) else {
            return false;
        };

        item.rotate_clockwise();
        drop(files);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_all_files(&self) -> bool {
        let mut files = self.files.borrow_mut();
        if files.is_empty() {
            return false;
        }

        for item in files.iter_mut() {
            item.rotate_clockwise();
        }
        drop(files);
        self.job.clear_last_output();
        true
    }

    pub fn reorder_file(&self, from: usize, to: usize) -> bool {
        self.move_file(from, to)
    }

    pub(super) fn remove_file(&self, index: usize) -> MergeRemoveUndo {
        let item = self.files.borrow_mut().remove(index);

        let password = (!self
            .files
            .borrow()
            .iter()
            .any(|file| file.path == item.path))
        .then(|| self.passwords.borrow_mut().remove(&item.path))
        .flatten();
        self.job.clear_last_output();
        MergeRemoveUndo {
            index,
            item,
            password,
        }
    }

    pub(super) fn restore_removed_file(&self, undo: MergeRemoveUndo) {
        if let Some(password) = undo.password {
            self.passwords
                .borrow_mut()
                .insert(undo.item.path.clone(), password);
        }
        self.files.borrow_mut().insert(undo.index, undo.item);
        self.job.clear_last_output();
    }

    pub fn duplicate_file(&self, index: usize) -> bool {
        let Some(item) = self.files.borrow().get(index).cloned() else {
            return false;
        };

        self.files.borrow_mut().insert(index + 1, item);
        self.job.clear_last_output();
        true
    }
}

impl CompressState {
    pub fn input_file(&self) -> Option<(PathBuf, Option<String>)> {
        self.file
            .borrow()
            .clone()
            .map(|path| (path, self.password.borrow().clone()))
    }

    pub fn finish_loading(
        &self,
        path: PathBuf,
        password: Option<String>,
        preview: Option<crate::preview::PagePreview>,
    ) {
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        *self.preview.borrow_mut() = preview;
    }
}

impl OrganizeState {
    pub fn load_document(
        &self,
        path: PathBuf,
        password: Option<String>,
        previews: crate::preview::DocumentPreviews,
    ) {
        let page_count = previews.page_count;
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        self.page_count.set(page_count);
        *self.previews.borrow_mut() = previews.previews;

        *self.page_order.borrow_mut() = original_page_order(page_count);
        self.job.clear_last_output();
    }

    pub(super) fn reset(&self) -> Option<OrganizeResetUndo> {
        let page_count = self.page_count.get();
        if self.file.borrow().is_none() || page_count == 0 {
            return None;
        }

        let original_page_order = original_page_order(page_count);
        let mut page_order = self.page_order.borrow_mut();
        if *page_order == original_page_order {
            return None;
        }

        let page_order = std::mem::replace(&mut *page_order, original_page_order);
        self.job.clear_last_output();
        Some(OrganizeResetUndo { page_order })
    }

    pub(super) fn restore_reset(&self, undo: OrganizeResetUndo) {
        *self.page_order.borrow_mut() = undo.page_order;
        self.job.clear_last_output();
    }

    pub fn selections(&self) -> Option<(PathBuf, Option<String>, Vec<crate::pdf::PageSelection>)> {
        let input_file = self.file.borrow().clone()?;
        let password = self.password.borrow().clone();
        let pages = self.page_order.borrow().clone();

        Some((input_file, password, pages))
    }

    pub fn move_page(&self, from: usize, to: usize) -> bool {
        let mut page_order = self.page_order.borrow_mut();
        if !move_vec_item(&mut page_order, from, to) {
            return false;
        }

        drop(page_order);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_page(&self, index: usize) -> bool {
        let mut pages = self.page_order.borrow_mut();
        let Some(page) = pages.get_mut(index) else {
            return false;
        };

        page.rotate_clockwise();
        drop(pages);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_all_pages(&self) -> bool {
        let mut pages = self.page_order.borrow_mut();
        if pages.is_empty() {
            return false;
        }

        for page in pages.iter_mut() {
            page.rotate_clockwise();
        }
        drop(pages);
        self.job.clear_last_output();
        true
    }

    pub fn reorder_page(&self, from: usize, to: usize) -> bool {
        self.move_page(from, to)
    }

    pub(super) fn remove_page(&self, index: usize) -> Option<OrganizeRemoveUndo> {
        if self.page_order.borrow().len() <= 1 {
            return None;
        }

        let page = self.page_order.borrow_mut().remove(index);
        self.job.clear_last_output();
        Some(OrganizeRemoveUndo { index, page })
    }

    pub(super) fn restore_removed_page(&self, undo: OrganizeRemoveUndo) {
        self.page_order.borrow_mut().insert(undo.index, undo.page);
        self.job.clear_last_output();
    }

    pub fn insert_blank_page_after(&self, index: usize) -> bool {
        let Some(page) = self.page_order.borrow().get(index).copied() else {
            return false;
        };

        self.page_order.borrow_mut().insert(
            index + 1,
            crate::pdf::PageSelection::blank_like_page(page.page_number, page.rotation),
        );
        self.job.clear_last_output();
        true
    }

    pub fn duplicate_page(&self, index: usize) -> bool {
        let Some(page) = self.page_order.borrow().get(index).copied() else {
            return false;
        };

        self.page_order.borrow_mut().insert(index + 1, page);
        self.job.clear_last_output();
        true
    }
}

impl ExtractState {
    pub fn clear_range_selection(&self) -> Vec<u32> {
        let mut changed = self
            .selected_pages
            .borrow()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        changed.extend(self.rotations.borrow().keys().copied());
        changed.sort_unstable();
        changed.dedup();
        self.selected_pages.borrow_mut().clear();
        self.rotations.borrow_mut().clear();
        changed
    }

    pub fn apply_range_selection(&self, pages: Vec<u32>) -> Vec<u32> {
        let pages = pages.into_iter().collect::<BTreeSet<_>>();
        let mut changed = self
            .selected_pages
            .borrow()
            .symmetric_difference(&pages)
            .copied()
            .collect::<Vec<_>>();
        let mut rotations = self.rotations.borrow_mut();
        rotations.retain(|page_number, _| {
            let retain = pages.contains(page_number);
            if !retain {
                changed.push(*page_number);
            }
            retain
        });
        drop(rotations);
        changed.sort_unstable();
        changed.dedup();
        *self.selected_pages.borrow_mut() = pages;
        changed
    }

    pub fn load_document(
        &self,
        path: PathBuf,
        password: Option<String>,
        previews: crate::preview::DocumentPreviews,
    ) {
        let page_count = previews.page_count;
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        self.page_count.set(page_count);
        *self.previews.borrow_mut() = previews.previews;
        let _ = self.clear_range_selection();
        self.job.clear_last_output();
    }

    pub fn selections_from_pages(
        &self,
        page_numbers: Vec<u32>,
    ) -> Option<(PathBuf, Option<String>, Vec<crate::pdf::PageSelection>)> {
        let input_file = self.file.borrow().clone()?;
        let password = self.password.borrow().clone();
        let rotations = self.rotations.borrow();
        let pages = page_numbers
            .into_iter()
            .map(|page_number| {
                crate::pdf::PageSelection::page(
                    page_number,
                    *rotations.get(&page_number).unwrap_or(&0),
                )
            })
            .collect();

        Some((input_file, password, pages))
    }

    pub fn toggle_page(&self, page_number: u32, selected: bool) -> bool {
        let mut pages = self.selected_pages.borrow_mut();

        let changed = if selected {
            pages.insert(page_number)
        } else {
            pages.remove(&page_number) | self.rotations.borrow_mut().remove(&page_number).is_some()
        };

        drop(pages);
        if changed {
            self.job.clear_last_output();
        }
        changed
    }

    pub fn rotate_page(&self, page_number: u32) -> bool {
        if !self.selected_pages.borrow().contains(&page_number) {
            return false;
        }

        rotate_entry(&self.rotations, page_number);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_selected_pages(&self) -> bool {
        let pages = self.selected_pages.borrow();
        if pages.is_empty() {
            return false;
        }

        for page_number in pages.iter() {
            rotate_entry(&self.rotations, *page_number);
        }
        self.job.clear_last_output();
        true
    }
}

impl SplitState {
    pub fn input_file(&self) -> Option<(PathBuf, Option<String>)> {
        self.file
            .borrow()
            .clone()
            .map(|path| (path, self.password.borrow().clone()))
    }

    pub fn finish_loading(
        &self,
        path: PathBuf,
        password: Option<String>,
        preview: Option<crate::preview::PagePreview>,
        page_count: usize,
    ) {
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        self.page_count.set(page_count);
        *self.preview.borrow_mut() = preview;
    }
}

impl MetadataState {
    pub fn input_file(&self) -> Option<(PathBuf, Option<String>)> {
        self.file
            .borrow()
            .clone()
            .map(|path| (path, self.password.borrow().clone()))
    }

    pub fn finish_loading(
        &self,
        path: PathBuf,
        password: Option<String>,
        preview: Option<crate::preview::PagePreview>,
    ) {
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        *self.preview.borrow_mut() = preview;
    }
}

impl WatermarkState {
    pub fn input_file(&self) -> Option<(PathBuf, Option<String>)> {
        self.file
            .borrow()
            .clone()
            .map(|path| (path, self.password.borrow().clone()))
    }

    pub fn image_file(&self) -> Option<PathBuf> {
        self.image_file.borrow().clone()
    }

    pub fn set_image_file(&self, path: PathBuf) {
        self.image_file.borrow_mut().replace(path);
        self.job.clear_last_output();
    }

    pub fn finish_loading(
        &self,
        path: PathBuf,
        password: Option<String>,
        preview: Option<crate::preview::PagePreview>,
        page_count: usize,
    ) {
        self.job.finish_loading();
        self.file.borrow_mut().replace(path);
        *self.password.borrow_mut() = password;
        self.page_count.set(page_count);
        *self.preview.borrow_mut() = preview;
    }
}

fn rotate_entry<Key>(rotations: &RefCell<BTreeMap<Key, i64>>, key: Key)
where
    Key: Ord,
{
    let mut rotations = rotations.borrow_mut();
    let rotation = rotate_clockwise(rotations.get(&key).copied().unwrap_or(0));
    if rotation == 0 {
        rotations.remove(&key);
    } else {
        rotations.insert(key, rotation);
    }
}

fn original_page_order(page_count: usize) -> Vec<crate::pdf::PageSelection> {
    (1..=page_count as u32)
        .map(|page| crate::pdf::PageSelection::page(page, 0))
        .collect()
}

fn rotate_clockwise(rotation: i64) -> i64 {
    (rotation + 90).rem_euclid(360)
}

fn move_vec_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from == to || from >= items.len() || to >= items.len() {
        return false;
    }

    let item = items.remove(from);
    items.insert(to, item);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        CompressState, ExtractState, JobState, MergeItem, MergeState, MetadataState, OrganizeState,
        OutputOptionsState, SaveOptionsState, SplitState, WatermarkState,
    };
    use std::path::PathBuf;

    fn document_previews(
        page_count: usize,
        rendered_pages: &[u32],
    ) -> crate::preview::DocumentPreviews {
        crate::preview::DocumentPreviews {
            page_count,
            previews: rendered_pages
                .iter()
                .map(|page_number| (*page_number, page_preview(*page_number)))
                .collect(),
        }
    }

    fn page_selection(page_number: u32) -> crate::pdf::PageSelection {
        crate::pdf::PageSelection::page(page_number, 0)
    }

    fn page_numbers(pages: &[crate::pdf::PageSelection]) -> Vec<u32> {
        pages.iter().map(|page| page.page_number).collect()
    }

    fn merge_item(path: &str) -> MergeItem {
        MergeItem::new(PathBuf::from(path))
    }

    fn merge_paths(items: &[MergeItem]) -> Vec<PathBuf> {
        items.iter().map(|item| item.path.clone()).collect()
    }

    fn page_preview(page_number: u32) -> crate::preview::PagePreview {
        crate::preview::PagePreview {
            page_number,
            image: crate::image::Argb32Image::new(
                1,
                1,
                4,
                vec![page_number as u8, page_number as u8, page_number as u8, 255],
            )
            .expect("test preview image should be valid"),
        }
    }

    #[test]
    fn job_state_tracks_loading_and_output() {
        let state = JobState::default();

        state.set_last_output(PathBuf::from("output.pdf"));
        assert!(state.has_last_output());
        assert!(!state.is_loading());
        assert!(!state.is_busy(false));

        state.begin_loading();
        assert!(state.is_loading());
        assert!(state.is_busy(false));
        assert!(!state.has_last_output());

        state.finish_loading();
        assert!(!state.is_loading());
        assert!(state.is_busy(true));
    }

    #[test]
    fn job_state_failed_loading_keeps_existing_output_policy() {
        let state = JobState::default();

        state.set_last_output(PathBuf::from("old-output.pdf"));
        state.begin_loading();
        state.finish_loading_failed();

        assert!(!state.is_loading());
        assert_eq!(state.last_output(), None);
    }

    #[test]
    fn output_options_state_builds_save_and_page_rewrite_options() {
        let state = OutputOptionsState::default();

        assert!(!state.modern_pdf());
        assert!(!state.normalize_page_size());
        assert!(!state.remove_metadata());

        state.set_modern_pdf(true);
        state.set_normalize_page_size(true);
        state.set_remove_metadata(true);

        let save_options = state.save_state().options();
        assert!(save_options.modern_pdf);
        assert!(save_options.remove_metadata);

        let output_options = state.options();
        assert!(output_options.normalize_page_size);
        assert_eq!(output_options.save.modern_pdf, save_options.modern_pdf);
        assert_eq!(
            output_options.save.remove_metadata,
            save_options.remove_metadata
        );
    }

    #[test]
    fn save_options_state_builds_save_options_only() {
        let state = SaveOptionsState::default();

        assert!(!state.modern_pdf());
        assert!(!state.remove_metadata());

        state.set_modern_pdf(true);
        state.set_remove_metadata(true);

        let options = state.options();
        assert!(options.modern_pdf);
        assert!(options.remove_metadata);
    }

    #[test]
    fn merge_move_file_moves_by_index_and_clears_output() {
        let state = MergeState::default();
        *state.files.borrow_mut() = vec![
            merge_item("first.pdf"),
            merge_item("second.pdf"),
            merge_item("third.pdf"),
        ];
        state.job.set_last_output(PathBuf::from("merged.pdf"));

        assert!(state.move_file(0, 2));

        assert_eq!(
            merge_paths(&state.files.borrow()),
            vec![
                PathBuf::from("second.pdf"),
                PathBuf::from("third.pdf"),
                PathBuf::from("first.pdf"),
            ]
        );
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn merge_move_file_rejects_invalid_indices() {
        let state = MergeState::default();
        *state.files.borrow_mut() = vec![merge_item("first.pdf"), merge_item("second.pdf")];
        state.job.set_last_output(PathBuf::from("merged.pdf"));

        assert!(!state.move_file(0, 0));
        assert!(!state.move_file(0, 2));
        assert!(!state.move_file(2, 0));

        assert_eq!(
            merge_paths(&state.files.borrow()),
            vec![PathBuf::from("first.pdf"), PathBuf::from("second.pdf")]
        );
        assert_eq!(state.job.last_output(), Some(PathBuf::from("merged.pdf")));
    }

    #[test]
    fn merge_tracks_passwords_in_pdf_inputs_and_clears_unused_passwords() {
        let state = MergeState::default();
        let locked = PathBuf::from("locked.pdf");
        let plain = PathBuf::from("plain.pdf");

        state.finish_loading(
            vec![locked.clone(), plain.clone()],
            Vec::new(),
            vec![
                (locked.clone(), Some("secret".to_string())),
                (plain.clone(), None),
            ],
        );

        let inputs = state.pdf_inputs();
        assert_eq!(inputs[0].path, locked);
        assert_eq!(inputs[0].password.as_deref(), Some("secret"));
        assert_eq!(inputs[1].path, plain);
        assert_eq!(inputs[1].password, None);

        state.remove_file(0);

        assert!(
            !state
                .passwords
                .borrow()
                .contains_key(&PathBuf::from("locked.pdf"))
        );
    }

    #[test]
    fn merge_clear_and_restore_recovers_cached_data_and_clears_output() {
        let state = MergeState::default();
        let locked = PathBuf::from("locked.pdf");
        state.finish_loading(
            vec![locked.clone()],
            vec![(locked.clone(), page_preview(1))],
            vec![(locked.clone(), Some("secret".to_string()))],
        );
        state.job.set_last_output(PathBuf::from("merged.pdf"));

        let undo = state.clear();

        assert!(state.files.borrow().is_empty());
        assert!(state.passwords.borrow().is_empty());
        assert!(state.previews.borrow().is_empty());
        assert_eq!(state.job.last_output(), None);

        state.job.set_last_output(PathBuf::from("stale.pdf"));
        state.restore_clear(undo);

        assert_eq!(merge_paths(&state.files.borrow()), vec![locked.clone()]);
        assert_eq!(
            state.passwords.borrow().get(&locked).map(String::as_str),
            Some("secret")
        );
        assert_eq!(
            state.previews.borrow()[&locked].image.pixels,
            vec![1, 1, 1, 255]
        );
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn merge_remove_and_restore_last_copy_recovers_password_and_clears_output() {
        let state = MergeState::default();
        let locked = PathBuf::from("locked.pdf");
        state.finish_loading(
            vec![locked.clone()],
            vec![(locked.clone(), page_preview(1))],
            vec![(locked.clone(), Some("secret".to_string()))],
        );

        let undo = state.remove_file(0);

        assert!(!state.passwords.borrow().contains_key(&locked));
        assert!(state.previews.borrow().contains_key(&locked));

        state.job.set_last_output(PathBuf::from("stale.pdf"));
        state.restore_removed_file(undo);

        assert_eq!(merge_paths(&state.files.borrow()), vec![locked.clone()]);
        assert_eq!(
            state.passwords.borrow().get(&locked).map(String::as_str),
            Some("secret")
        );
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn merge_remove_and_restore_duplicate_keeps_shared_cached_data() {
        let state = MergeState::default();
        let locked = PathBuf::from("locked.pdf");
        state.finish_loading(
            vec![locked.clone()],
            vec![(locked.clone(), page_preview(1))],
            vec![(locked.clone(), Some("secret".to_string()))],
        );
        assert!(state.duplicate_file(0));

        let undo = state.remove_file(0);

        assert!(state.passwords.borrow().contains_key(&locked));
        assert!(state.previews.borrow().contains_key(&locked));

        state.restore_removed_file(undo);

        assert_eq!(
            merge_paths(&state.files.borrow()),
            vec![locked.clone(), locked]
        );
        assert_eq!(state.previews.borrow().len(), 1);
    }

    #[test]
    fn merge_rotates_duplicated_files_independently() {
        let state = MergeState::default();
        state.add_files(vec![PathBuf::from("input.pdf")]);

        assert!(state.duplicate_file(0));
        assert!(state.rotate_file(1));

        let inputs = state.pdf_inputs();
        assert_eq!(inputs[0].rotation, 0);
        assert_eq!(inputs[1].rotation, 90);
    }

    #[test]
    fn merge_keeps_rotations_with_move_remove_and_duplicate() {
        let state = MergeState::default();
        state.add_files(vec![
            PathBuf::from("first.pdf"),
            PathBuf::from("second.pdf"),
            PathBuf::from("third.pdf"),
        ]);

        assert!(state.rotate_file(1));
        assert!(state.rotate_file(2));
        assert!(state.move_file(1, 0));
        assert!(state.duplicate_file(0));
        assert!(state.rotate_file(1));
        state.remove_file(2);

        let inputs = state.pdf_inputs();
        assert_eq!(
            inputs
                .iter()
                .map(|input| input.path.clone())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("second.pdf"),
                PathBuf::from("second.pdf"),
                PathBuf::from("third.pdf"),
            ]
        );
        assert_eq!(
            inputs
                .iter()
                .map(|input| input.rotation)
                .collect::<Vec<_>>(),
            vec![90, 180, 90]
        );
    }

    #[test]
    fn organize_move_page_moves_by_index_and_clears_output() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() =
            vec![page_selection(1), page_selection(2), page_selection(3)];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(state.move_page(2, 0));

        assert_eq!(page_numbers(&state.page_order.borrow()), vec![3, 1, 2]);
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn organize_move_page_rejects_invalid_indices() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![page_selection(1), page_selection(2)];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(!state.move_page(1, 1));
        assert!(!state.move_page(0, 2));
        assert!(!state.move_page(2, 0));

        assert_eq!(page_numbers(&state.page_order.borrow()), vec![1, 2]);
        assert_eq!(
            state.job.last_output(),
            Some(PathBuf::from("organized.pdf"))
        );
    }

    #[test]
    fn organize_uses_document_page_count_even_when_previews_are_missing() {
        let state = OrganizeState::default();

        state.load_document(
            PathBuf::from("input.pdf"),
            None,
            document_previews(3, &[1, 3]),
        );

        assert_eq!(state.page_count.get(), 3);
        assert_eq!(page_numbers(&state.page_order.borrow()), vec![1, 2, 3]);
        assert!(!state.previews.borrow().contains_key(&2));
    }

    #[test]
    fn organize_duplicates_and_inserts_blank_pages() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![page_selection(1), page_selection(2)];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(state.duplicate_page(0));
        assert!(state.insert_blank_page_after(1));

        let pages = state.page_order.borrow();
        assert_eq!(page_numbers(&pages), vec![1, 1, 1, 2]);
        assert!(!pages[1].is_blank());
        assert!(pages[2].is_blank());
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn organize_blank_pages_keep_rotation_when_rotated_and_reused() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![crate::pdf::PageSelection::page(1, 90)];

        assert!(state.insert_blank_page_after(0));
        assert!(state.rotate_page(1));
        assert!(state.insert_blank_page_after(1));

        let pages = state.page_order.borrow();
        assert_eq!(page_numbers(&pages), vec![1, 1, 1]);
        assert_eq!(
            pages.iter().map(|page| page.rotation).collect::<Vec<_>>(),
            vec![90, 180, 180]
        );
        assert!(!pages[0].is_blank());
        assert!(pages[1].is_blank());
        assert!(pages[2].is_blank());
    }

    #[test]
    fn organize_stores_password_with_page_selections() {
        let state = OrganizeState::default();

        state.load_document(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            document_previews(2, &[1, 2]),
        );

        let (path, password, pages) = state.selections().unwrap();
        assert_eq!(path, PathBuf::from("locked.pdf"));
        assert_eq!(password.as_deref(), Some("secret"));
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn organize_reset_and_restore_recovers_staged_pages_and_clears_output() {
        let state = OrganizeState::default();
        state.load_document(
            PathBuf::from("input.pdf"),
            None,
            document_previews(3, &[1, 2, 3]),
        );
        assert!(state.rotate_page(1));
        assert!(state.duplicate_page(1));
        assert!(state.insert_blank_page_after(2));
        let staged_pages = state.page_order.borrow().clone();
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        let undo = state.reset().unwrap();

        assert_eq!(
            state.page_order.borrow().as_slice(),
            &[page_selection(1), page_selection(2), page_selection(3)]
        );
        assert_eq!(state.job.last_output(), None);

        state.job.set_last_output(PathBuf::from("stale.pdf"));
        state.restore_reset(undo);

        assert_eq!(*state.page_order.borrow(), staged_pages);
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn organize_remove_and_restore_recovers_position_and_clears_output() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![
            page_selection(1),
            crate::pdf::PageSelection::blank_like_page(1, 90),
            page_selection(2),
        ];

        let undo = state.remove_page(1).unwrap();

        assert_eq!(page_numbers(&state.page_order.borrow()), vec![1, 2]);

        state.job.set_last_output(PathBuf::from("stale.pdf"));
        state.restore_removed_page(undo);

        let pages = state.page_order.borrow();
        assert_eq!(page_numbers(&pages), vec![1, 1, 2]);
        assert!(pages[1].is_blank());
        assert_eq!(pages[1].rotation, 90);
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn organize_remove_rejects_final_page() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![page_selection(1)];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(state.remove_page(0).is_none());

        assert_eq!(page_numbers(&state.page_order.borrow()), vec![1]);
        assert_eq!(
            state.job.last_output(),
            Some(PathBuf::from("organized.pdf"))
        );
    }

    #[test]
    fn extract_uses_document_page_count_even_when_previews_are_missing() {
        let state = ExtractState::default();

        state.load_document(
            PathBuf::from("input.pdf"),
            None,
            document_previews(4, &[2, 4]),
        );

        assert_eq!(state.page_count.get(), 4);
        assert_eq!(state.previews.borrow().len(), 2);
        assert!(!state.previews.borrow().contains_key(&1));
    }

    #[test]
    fn extract_stores_password_with_page_selections() {
        let state = ExtractState::default();

        state.load_document(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            document_previews(2, &[1, 2]),
        );

        let (path, password, pages) = state.selections_from_pages(vec![2]).unwrap();
        assert_eq!(path, PathBuf::from("locked.pdf"));
        assert_eq!(password.as_deref(), Some("secret"));
        assert_eq!(pages[0].page_number, 2);
    }

    #[test]
    fn single_file_states_store_and_overwrite_passwords() {
        let compress = CompressState::default();
        compress.finish_loading(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            None,
        );
        assert_eq!(
            compress.input_file().unwrap(),
            (PathBuf::from("locked.pdf"), Some("secret".to_string()))
        );

        compress.finish_loading(PathBuf::from("plain.pdf"), None, None);
        assert_eq!(
            compress.input_file().unwrap(),
            (PathBuf::from("plain.pdf"), None)
        );

        let split = SplitState::default();
        split.finish_loading(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            None,
            3,
        );
        assert_eq!(
            split.input_file().unwrap(),
            (PathBuf::from("locked.pdf"), Some("secret".to_string()))
        );

        split.finish_loading(PathBuf::from("plain.pdf"), None, None, 2);
        assert_eq!(
            split.input_file().unwrap(),
            (PathBuf::from("plain.pdf"), None)
        );

        let metadata = MetadataState::default();
        metadata.finish_loading(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            None,
        );
        assert_eq!(
            metadata.input_file().unwrap(),
            (PathBuf::from("locked.pdf"), Some("secret".to_string()))
        );

        metadata.finish_loading(PathBuf::from("plain.pdf"), None, None);
        assert_eq!(
            metadata.input_file().unwrap(),
            (PathBuf::from("plain.pdf"), None)
        );

        let watermark = WatermarkState::default();
        watermark.finish_loading(
            PathBuf::from("locked.pdf"),
            Some("secret".to_string()),
            None,
            3,
        );
        assert_eq!(
            watermark.input_file().unwrap(),
            (PathBuf::from("locked.pdf"), Some("secret".to_string()))
        );

        watermark.finish_loading(PathBuf::from("plain.pdf"), None, None, 2);
        assert_eq!(
            watermark.input_file().unwrap(),
            (PathBuf::from("plain.pdf"), None)
        );
    }

    #[test]
    fn extract_selection_is_sorted_and_deduplicated() {
        let state = ExtractState::default();

        let _ = state.apply_range_selection(vec![3, 1, 3, 2]);

        assert_eq!(
            state
                .selected_pages
                .borrow()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn extract_selection_updates_report_no_op_changes() {
        let state = ExtractState::default();

        assert!(state.toggle_page(1, true));
        assert!(!state.toggle_page(1, true));
        assert_eq!(state.apply_range_selection(vec![1, 2]), vec![2]);
        assert!(state.apply_range_selection(vec![1, 2]).is_empty());
        assert_eq!(state.clear_range_selection(), vec![1, 2]);
        assert!(state.clear_range_selection().is_empty());
    }

    #[test]
    fn extract_range_selection_reports_only_changed_pages() {
        let state = ExtractState::default();

        assert_eq!(state.apply_range_selection(vec![1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(state.apply_range_selection(vec![2, 3, 4]), vec![1, 4]);
    }
}
