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

#[derive(Debug, Default)]
pub struct OutputOptionsState {
    normalize_page_size: Cell<bool>,
    remove_metadata: Cell<bool>,
}

impl OutputOptionsState {
    pub fn set_normalize_page_size(&self, active: bool) {
        self.normalize_page_size.set(active);
    }

    pub fn set_remove_metadata(&self, active: bool) {
        self.remove_metadata.set(active);
    }

    pub fn options(&self) -> crate::pdf::PdfOutputOptions {
        crate::pdf::PdfOutputOptions {
            normalize_page_size: self.normalize_page_size.get(),
            remove_metadata: self.remove_metadata.get(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MergeState {
    pub files: RefCell<Vec<PathBuf>>,
    pub passwords: RefCell<BTreeMap<PathBuf, String>>,
    pub rotations: RefCell<BTreeMap<PathBuf, i64>>,
    pub previews: RefCell<BTreeMap<PathBuf, crate::preview::PagePreview>>,
    pub options: OutputOptionsState,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct CompressState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub preview: RefCell<Option<crate::preview::PagePreview>>,
    pub job: JobState,
}

#[derive(Debug, Default)]
pub struct OrganizeState {
    pub file: RefCell<Option<PathBuf>>,
    pub password: RefCell<Option<String>>,
    pub page_count: Cell<usize>,
    pub previews: RefCell<BTreeMap<u32, crate::preview::PagePreview>>,
    pub page_order: RefCell<Vec<u32>>,
    pub rotations: RefCell<BTreeMap<u32, i64>>,
    pub options: OutputOptionsState,
    pub job: JobState,
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
    pub job: JobState,
}

impl MergeState {
    pub fn clear(&self) {
        self.files.borrow_mut().clear();
        self.passwords.borrow_mut().clear();
        self.rotations.borrow_mut().clear();
        self.previews.borrow_mut().clear();
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
        self.files.borrow_mut().extend(paths);
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
        self.files.borrow_mut().extend(paths);
    }

    pub fn pdf_inputs(&self) -> Vec<crate::pdf::PdfInput> {
        let rotations = self.rotations.borrow();
        let passwords = self.passwords.borrow();
        self.files
            .borrow()
            .iter()
            .map(|path| crate::pdf::PdfInput {
                path: path.clone(),
                password: passwords.get(path).cloned(),
                rotation: *rotations.get(path).unwrap_or(&0),
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
        let Some(path) = self.files.borrow().get(index).cloned() else {
            return false;
        };
        rotate_entry(&self.rotations, path);
        self.job.clear_last_output();
        true
    }

    pub fn rotate_all_files(&self) -> bool {
        let files = self.files.borrow();
        if files.is_empty() {
            return false;
        }

        for path in files.iter().cloned().collect::<BTreeSet<_>>() {
            rotate_entry(&self.rotations, path);
        }
        self.job.clear_last_output();
        true
    }

    pub fn reorder_file(&self, from: usize, to: usize) -> bool {
        self.move_file(from, to)
    }

    pub fn remove_file(&self, index: usize) {
        let path = self.files.borrow_mut().remove(index);
        if !self.files.borrow().contains(&path) {
            self.passwords.borrow_mut().remove(&path);
            self.rotations.borrow_mut().remove(&path);
        }
        self.job.clear_last_output();
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

        let mut page_order = self.page_order.borrow_mut();
        page_order.clear();
        page_order.extend(1..=page_count as u32);

        self.rotations.borrow_mut().clear();
        self.job.clear_last_output();
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
        self.job.clear_last_output();
        true
    }

    pub fn selections(&self) -> Option<(PathBuf, Option<String>, Vec<crate::pdf::PageSelection>)> {
        let input_file = self.file.borrow().clone()?;
        let password = self.password.borrow().clone();
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

    pub fn rotate_page(&self, page_number: u32) {
        rotate_entry(&self.rotations, page_number);
        self.job.clear_last_output();
    }

    pub fn rotate_all_pages(&self) -> bool {
        let pages = self.page_order.borrow();
        if pages.is_empty() {
            return false;
        }

        for page_number in pages.iter() {
            rotate_entry(&self.rotations, *page_number);
        }
        self.job.clear_last_output();
        true
    }

    pub fn reorder_page(&self, dragged_page: u32, target_page: u32) -> bool {
        if dragged_page == target_page {
            return false;
        }

        let pages = self.page_order.borrow();
        let Some(from) = pages.iter().position(|page| *page == dragged_page) else {
            return false;
        };
        let Some(to) = pages.iter().position(|page| *page == target_page) else {
            return false;
        };

        drop(pages);
        self.move_page(from, to)
    }

    pub fn remove_page(&self, index: usize) -> bool {
        if self.page_order.borrow().len() <= 1 {
            return false;
        }

        let page_number = self.page_order.borrow_mut().remove(index);
        self.rotations.borrow_mut().remove(&page_number);
        self.job.clear_last_output();
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
        *self.selected_pages.borrow_mut() = pages.into_iter().collect();
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
        self.clear_range_selection();
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
            .map(|page_number| crate::pdf::PageSelection {
                page_number,
                rotation: *rotations.get(&page_number).unwrap_or(&0),
            })
            .collect();

        Some((input_file, password, pages))
    }

    pub fn toggle_page(&self, page_number: u32, selected: bool) {
        let mut pages = self.selected_pages.borrow_mut();

        if selected {
            pages.insert(page_number);
        } else {
            pages.remove(&page_number);
            self.rotations.borrow_mut().remove(&page_number);
        }

        drop(pages);
        self.job.clear_last_output();
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
    use super::{CompressState, ExtractState, JobState, MergeState, OrganizeState, SplitState};
    use std::path::PathBuf;

    fn document_previews(
        page_count: usize,
        rendered_pages: &[u32],
    ) -> crate::preview::DocumentPreviews {
        crate::preview::DocumentPreviews {
            page_count,
            previews: rendered_pages
                .iter()
                .map(|page_number| {
                    (
                        *page_number,
                        crate::preview::PagePreview {
                            page_number: *page_number,
                            png_data: Vec::new(),
                        },
                    )
                })
                .collect(),
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
    fn merge_move_file_moves_by_index_and_clears_output() {
        let state = MergeState::default();
        *state.files.borrow_mut() = vec![
            PathBuf::from("first.pdf"),
            PathBuf::from("second.pdf"),
            PathBuf::from("third.pdf"),
        ];
        state.job.set_last_output(PathBuf::from("merged.pdf"));

        assert!(state.move_file(0, 2));

        assert_eq!(
            *state.files.borrow(),
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
        *state.files.borrow_mut() = vec![PathBuf::from("first.pdf"), PathBuf::from("second.pdf")];
        state.job.set_last_output(PathBuf::from("merged.pdf"));

        assert!(!state.move_file(0, 0));
        assert!(!state.move_file(0, 2));
        assert!(!state.move_file(2, 0));

        assert_eq!(
            *state.files.borrow(),
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

        assert!(!state
            .passwords
            .borrow()
            .contains_key(&PathBuf::from("locked.pdf")));
    }

    #[test]
    fn organize_move_page_moves_by_index_and_clears_output() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![1, 2, 3];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(state.move_page(2, 0));

        assert_eq!(*state.page_order.borrow(), vec![3, 1, 2]);
        assert_eq!(state.job.last_output(), None);
    }

    #[test]
    fn organize_move_page_rejects_invalid_indices() {
        let state = OrganizeState::default();
        *state.page_order.borrow_mut() = vec![1, 2];
        state.job.set_last_output(PathBuf::from("organized.pdf"));

        assert!(!state.move_page(1, 1));
        assert!(!state.move_page(0, 2));
        assert!(!state.move_page(2, 0));

        assert_eq!(*state.page_order.borrow(), vec![1, 2]);
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
        assert_eq!(*state.page_order.borrow(), vec![1, 2, 3]);
        assert!(!state.previews.borrow().contains_key(&2));
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
    }

    #[test]
    fn extract_selection_is_sorted_and_deduplicated() {
        let state = ExtractState::default();

        state.apply_range_selection(vec![3, 1, 3, 2]);

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
}
