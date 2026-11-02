//! Windows half of the frame publication (format and protocol in
//! `bwrapi_engine::publish`): the game thread's [`Publisher`] and an outside
//! reader's [`GamePublication`]. Each reader slot has an auto-reset event the
//! game sets after every publication, so a reader sleeps until a frame is
//! published instead of polling.

use std::mem::size_of;
use std::ptr::null;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::AtomicU32;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bwrapi_engine::publish::{Control, Publication, Region, Writer, READERS, VERSION};
use bwrapi_engine::{CommandBasis, Frame};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery, FILE_MAP_ALL_ACCESS,
    FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, GetExitCodeProcess, OpenEventW, OpenProcess, SetEvent, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZATION_SYNCHRONIZE,
};

use crate::{hook_state, publish_data_name, publish_event_name, publish_name, GameStatus};

/// Publications between two looks at whether the readers' processes still
/// run; a reader that exits without detaching would keep the game copying.
const PRUNE_EVERY: u32 = 256;

/// A view of a named mapping, unmapped when dropped.
struct View {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

// A view is an address range of this process; any thread may unmap it.
unsafe impl Send for View {}

impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view);
            if self.handle != 0 {
                CloseHandle(self.handle);
            }
        }
    }
}

fn create_view(name: &[u16], len: usize) -> Option<View> {
    let handle = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            null(),
            PAGE_READWRITE,
            (len as u64 >> 32) as u32,
            len as u32,
            name.as_ptr(),
        )
    };
    if handle == 0 {
        return None;
    }
    let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, len) };
    if view.Value.is_null() {
        unsafe { CloseHandle(handle) };
        return None;
    }
    Some(View { handle, view })
}

/// Game-thread side, created with the frame hook. Costs nothing per frame
/// while no reader is attached: the hook asks [`Publisher::readers`] first.
pub(crate) struct Publisher {
    control: &'static Control,
    events: [HANDLE; READERS],
    /// `Control::change` the last publication answered; while it differs the
    /// watcher posts a frame message even when the frame stands still.
    answered: AtomicU32,
    state: Mutex<State>,
}

struct State {
    writer: Writer,
    /// The data mapping publications go to; replaced when one outgrows it.
    data: Option<View>,
    /// Whether the last publication was of a match, so leaving the match
    /// publishes once.
    in_game: bool,
    publications: u32,
}

impl Publisher {
    /// Creates `Local\bwrapi_game_publish_<pid>` and the reader events. The
    /// control mapping lives as long as the game, so its handle stays open.
    pub fn create() -> Option<Self> {
        let pid = unsafe { GetCurrentProcessId() };
        let view = create_view(&publish_name(pid), size_of::<Control>())?;
        let control = unsafe { &*view.view.Value.cast::<Control>() };
        std::mem::forget(view);
        let mut events = [0; READERS];
        for (slot, event) in events.iter_mut().enumerate() {
            *event = unsafe { CreateEventW(null(), 0, 0, publish_event_name(pid, slot).as_ptr()) };
            if *event == 0 {
                return None;
            }
        }
        control.version.store(VERSION, Release);
        Some(Self {
            control,
            events,
            answered: AtomicU32::new(0),
            state: Mutex::new(State { writer: Writer::default(), data: None, in_game: false, publications: 0 }),
        })
    }

    /// Whether a reader attached, resubscribed or left since the last
    /// publication.
    pub fn pending(&self) -> bool {
        self.control.change.load(Acquire) != self.answered.load(Relaxed)
    }

    /// Union of the attached readers' sources and the change count it is
    /// for, or `None` when no reader is attached.
    pub fn readers(&self) -> Option<(u64, u32)> {
        let change = self.control.change.load(Acquire);
        let sources = self.control.readers();
        if sources.is_none() {
            self.answered.store(change, Relaxed);
        }
        sources.map(|sources| (sources, change))
    }

    /// Publishes that no match runs, once after a match and once for each
    /// reader change.
    pub fn publish_menu(&self) {
        let in_game = self.state.lock().map_or(true, |state| state.in_game);
        if !in_game && !self.pending() {
            return;
        }
        if let Some((sources, change)) = self.readers() {
            self.publish(None, sources, None, change);
        }
    }

    /// Publishes `frame`'s tables of `sources` (or the menu when `None`) and
    /// wakes the readers.
    pub fn publish(&self, frame: Option<&Frame>, sources: u64, basis: Option<&CommandBasis>, change: u32) {
        let started = Instant::now();
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let pid = unsafe { GetCurrentProcessId() };
        let State { writer, data, .. } = &mut *state;
        let mut map = |generation: u32, len: usize| {
            let view = create_view(&publish_data_name(pid, generation), len)?;
            let region = unsafe { Region::new(view.view.Value.cast(), len) };
            // Readers still holding the old mapping keep it alive.
            *data = Some(view);
            Some(region)
        };
        if !unsafe { writer.publish(self.control, frame, sources, sources, basis, &mut map) } {
            self.answered.store(change, Relaxed);
            return;
        }
        state.in_game = frame.is_some();
        self.answered.store(change, Relaxed);
        for (reader, event) in self.control.readers.iter().zip(self.events) {
            if reader.owner.load(Relaxed) != 0 {
                unsafe { SetEvent(event) };
            }
        }
        state.publications = state.publications.wrapping_add(1);
        if state.publications % PRUNE_EVERY == 0 {
            self.prune();
        }
        self.control.publish_us.store(started.elapsed().as_micros() as u32, Relaxed);
    }

    /// Frees the slots of readers whose process exited.
    fn prune(&self) {
        for reader in &self.control.readers {
            let owner = reader.owner.load(Acquire);
            if owner != 0 && !running(owner) && reader.owner.compare_exchange(owner, 0, Release, Relaxed).is_ok() {
                reader.sources.store(0, Release);
                self.control.change.fetch_add(1, Release);
            }
        }
    }
}

/// Whether process `pid` still runs. A process that cannot be opened for
/// another reason (access) counts as running.
fn running(pid: u32) -> bool {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process == 0 {
        return unsafe { GetLastError() } != ERROR_INVALID_PARAMETER;
    }
    let mut code = 0;
    let read = unsafe { GetExitCodeProcess(process, &mut code) };
    unsafe { CloseHandle(process) };
    read == 0 || code == STILL_ACTIVE as u32
}

/// An outside reader's attachment to the frames game `pid` publishes. Holds
/// a reader slot until dropped; the game reads and publishes only while some
/// reader is attached.
pub struct GamePublication {
    control: View,
    slot: usize,
    event: HANDLE,
    game: u32,
    /// Data mapping of the generation last read.
    data: Option<(u32, View, Region)>,
}

// The views are shared memory; the publication protocol orders every access.
unsafe impl Send for GamePublication {}

impl GamePublication {
    /// Attaches to game `pid`; `None` when it runs no in-game DLL or every
    /// reader slot is taken, so the caller reads the game from outside.
    pub fn open(pid: u32) -> Option<Self> {
        if !GameStatus::open(pid).is_some_and(|status| status.hook_state == hook_state::INSTALLED) {
            return None;
        }
        Self::attach(pid)
    }

    fn attach(pid: u32) -> Option<Self> {
        let name = publish_name(pid);
        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) };
        if handle == 0 {
            return None;
        }
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<Control>()) };
        unsafe { CloseHandle(handle) };
        if view.Value.is_null() {
            return None;
        }
        let control = View { handle: 0, view };
        let block = unsafe { &*control.view.Value.cast::<Control>() };
        if block.version.load(Acquire) != VERSION {
            return None;
        }
        let slot = block.attach(unsafe { GetCurrentProcessId() })?;
        let event = unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, 0, publish_event_name(pid, slot).as_ptr()) };
        if event == 0 {
            block.detach(slot);
            return None;
        }
        Some(Self { control, slot, event, game: pid, data: None })
    }
}

impl Publication for GamePublication {
    fn control(&self) -> &Control {
        unsafe { &*self.control.view.Value.cast::<Control>() }
    }

    fn data(&mut self, generation: u32) -> Option<Region> {
        if let Some((held, _, region)) = &self.data {
            if *held == generation {
                return Some(*region);
            }
        }
        let name = publish_data_name(self.game, generation);
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if handle == 0 {
            return None;
        }
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) };
        unsafe { CloseHandle(handle) };
        if view.Value.is_null() {
            return None;
        }
        let view = View { handle: 0, view };
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let queried =
            unsafe { VirtualQuery(view.view.Value, &mut info, size_of::<MEMORY_BASIC_INFORMATION>()) };
        if queried == 0 {
            return None;
        }
        // Only the game writes the data mapping; this view is read-only.
        let region = unsafe { Region::new(view.view.Value.cast(), info.RegionSize) };
        self.data = Some((generation, view, region));
        Some(region)
    }

    fn wait(&self, timeout: Duration) {
        let ms = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        unsafe { WaitForSingleObject(self.event, ms) };
    }

    fn slot(&self) -> usize {
        self.slot
    }
}

impl Drop for GamePublication {
    fn drop(&mut self) {
        self.control().detach(self.slot);
        unsafe { CloseHandle(self.event) };
    }
}

#[cfg(test)]
mod tests {
    use bwrapi_engine::fake::FakeMemory;
    use bwrapi_engine::tables::{source, UnitRow};
    use bwrapi_engine::{Engine, Error, Frame};

    use super::*;

    fn frame(game_frame: u32, units: usize) -> Frame {
        let mut frame = Frame { match_id: 1, game_frame, sources: source::UNITS, ..Frame::default() };
        for index in 0..units {
            frame.units.push(UnitRow { id: index as u64 + 1, x: game_frame as i16, ..UnitRow::UNKNOWN });
        }
        frame
    }

    /// One test: the named objects are per process.
    #[test]
    fn a_reader_maps_what_the_game_thread_published() {
        let publisher = Publisher::create().expect("publication mappings");
        let reader = GamePublication::attach(std::process::id()).expect("reader slot");
        let mut engine = Engine::with_memory(Box::new(FakeMemory::new())).unwrap();
        engine.subscribe(source::UNITS);
        engine.read_published(Box::new(reader));
        assert!(publisher.pending());
        let (sources, change) = publisher.readers().expect("an attached reader");
        assert_eq!(sources, source::UNITS);

        let small = frame(5, 10);
        publisher.publish(Some(&small), sources, None, change);
        assert!(!publisher.pending());
        assert_eq!(engine.acquire().unwrap().units, small.units);
        assert_eq!(publisher.control.generation.load(Acquire), 1);

        // Outgrows the first mapping: the reader follows to the next one.
        let large = frame(6, 3400);
        publisher.publish(Some(&large), sources, None, change);
        assert_eq!(publisher.control.generation.load(Acquire), 2);
        let read = engine.acquire().unwrap();
        assert_eq!((read.game_frame, &read.units), (6, &large.units));

        assert_eq!(engine.step(Duration::from_millis(5)), Err(Error::Timeout));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                publisher.publish(Some(&frame(7, 3)), sources, None, change);
            });
            assert_eq!(engine.step(Duration::from_secs(5)), Ok(()));
        });
        assert_eq!(engine.acquire().unwrap().game_frame, 7);

        publisher.publish_menu();
        assert_eq!(engine.acquire().err(), Some(Error::NotInGame));
        drop(engine);
        assert_eq!(publisher.readers(), None);
    }
}
