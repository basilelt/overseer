//! WebKitGTK 2.54.0 and 2.54.1 crash the whole app (SIGSEGV in the UI process) on any paste from a
//! selection nobody owns: GTK reports its targets as (NULL, -1) and `Clipboard::formats()` walks
//! them as a span of `(size_t)-1` atoms (https://bugs.webkit.org/show_bug.cgi?id=324466, fixed in
//! 2.54.2). A middle-click or Ctrl+V with an empty clipboard was enough. Keep CLIPBOARD and PRIMARY
//! owned: claim an empty one when nobody answers, at startup, on focus and a moment after an owner
//! change, which leaves a clipboard manager time to take over from an owner that quit.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(250);

pub(crate) trait Selection: 'static {
    fn probe(&self, done: Box<dyn FnOnce(bool)>);
    fn claim(&self);
    fn on_owner_change(&self, f: Box<dyn Fn()>);
    fn later(&self, delay: Duration, f: Box<dyn FnOnce()>);
}

pub(crate) fn check<S: Selection>(s: &Rc<S>) {
    let c = s.clone();
    s.probe(Box::new(move |owned| {
        if !owned {
            c.claim();
        }
    }));
}

pub(crate) fn guard<S: Selection>(s: Rc<S>) {
    check(&s);
    let c = s.clone();
    let pending = Rc::new(Cell::new(false));
    s.on_owner_change(Box::new(move || {
        if pending.replace(true) {
            return;
        }
        let (again, pending) = (c.clone(), pending.clone());
        c.later(
            SETTLE,
            Box::new(move || {
                pending.set(false);
                check(&again);
            }),
        );
    }));
}

#[cfg(target_os = "linux")]
mod gtk_impl {
    use super::Selection;
    use gtk::glib::object::ObjectExt;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub(crate) struct Gtk(gtk::Clipboard);

    impl Selection for Gtk {
        fn probe(&self, done: Box<dyn FnOnce(bool)>) {
            self.0
                .request_contents(&gtk::gdk::Atom::intern("TARGETS"), move |_, d| {
                    done(d.length() >= 0)
                });
        }

        fn claim(&self) {
            self.0.set_text("");
        }

        fn on_owner_change(&self, f: Box<dyn Fn()>) {
            self.0.connect_local("owner-change", false, move |_| {
                f();
                None
            });
        }

        fn later(&self, delay: std::time::Duration, f: Box<dyn FnOnce()>) {
            gtk::glib::timeout_add_local_once(delay, f);
        }
    }

    thread_local! {
        static GUARDED: RefCell<Vec<Rc<Gtk>>> = const { RefCell::new(Vec::new()) };
    }

    pub fn install() {
        for atom in [gtk::gdk::SELECTION_CLIPBOARD, gtk::gdk::SELECTION_PRIMARY] {
            let s = Rc::new(Gtk(gtk::Clipboard::get(&atom)));
            super::guard(s.clone());
            GUARDED.with(|g| g.borrow_mut().push(s));
        }
    }

    pub fn recheck() {
        GUARDED.with(|g| g.borrow().iter().for_each(super::check));
    }
}

#[cfg(target_os = "linux")]
pub use gtk_impl::{install, recheck};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    type Timer = (Duration, Box<dyn FnOnce()>);

    #[derive(Default)]
    struct Fake {
        owned: Cell<bool>,
        probes: Cell<u32>,
        claims: Cell<u32>,
        listener: RefCell<Option<Box<dyn Fn()>>>,
        timers: RefCell<Vec<Timer>>,
    }

    impl Fake {
        fn owned() -> Rc<Fake> {
            Rc::new(Fake {
                owned: Cell::new(true),
                ..Default::default()
            })
        }

        fn owner_changes(&self, owned: bool) {
            self.owned.set(owned);
            if let Some(f) = self.listener.borrow().as_ref() {
                f();
            }
        }

        fn settle(&self) {
            let due: Vec<_> = self.timers.borrow_mut().drain(..).collect();
            for (delay, f) in due {
                assert_eq!(delay, SETTLE);
                f();
            }
        }
    }

    impl Selection for Fake {
        fn probe(&self, done: Box<dyn FnOnce(bool)>) {
            self.probes.set(self.probes.get() + 1);
            done(self.owned.get());
        }

        fn claim(&self) {
            self.claims.set(self.claims.get() + 1);
            self.owned.set(true);
        }

        fn on_owner_change(&self, f: Box<dyn Fn()>) {
            *self.listener.borrow_mut() = Some(f);
        }

        fn later(&self, delay: Duration, f: Box<dyn FnOnce()>) {
            self.timers.borrow_mut().push((delay, f));
        }
    }

    #[test]
    fn an_unowned_selection_is_claimed_at_install() {
        let s = Rc::new(Fake::default());
        guard(s.clone());
        assert_eq!((s.probes.get(), s.claims.get()), (1, 1));
        assert!(s.owned.get());
    }

    #[test]
    fn an_owned_selection_is_left_to_its_owner() {
        let s = Fake::owned();
        guard(s.clone());
        s.owner_changes(true);
        s.settle();
        assert_eq!((s.probes.get(), s.claims.get()), (2, 0));
    }

    #[test]
    fn the_selection_is_claimed_again_a_moment_after_its_owner_goes_away() {
        let s = Fake::owned();
        guard(s.clone());
        s.owner_changes(false);
        assert_eq!(s.claims.get(), 0);
        s.settle();
        assert_eq!(s.claims.get(), 1);
        assert!(s.owned.get());
        s.owner_changes(true);
        s.owner_changes(false);
        s.settle();
        assert_eq!(s.claims.get(), 2);
    }

    #[test]
    fn a_clipboard_manager_taking_over_meanwhile_keeps_the_selection() {
        let s = Fake::owned();
        guard(s.clone());
        s.owner_changes(false);
        s.owner_changes(true);
        s.settle();
        assert_eq!(s.claims.get(), 0);
    }

    #[test]
    fn a_burst_of_owner_changes_is_probed_once() {
        let s = Fake::owned();
        guard(s.clone());
        for _ in 0..50 {
            s.owner_changes(true);
        }
        assert_eq!(s.timers.borrow().len(), 1);
        s.settle();
        assert_eq!(s.probes.get(), 2);
        s.owner_changes(true);
        assert_eq!(s.timers.borrow().len(), 1);
    }

    #[test]
    fn a_recheck_claims_only_a_selection_left_without_owner() {
        let s = Fake::owned();
        check(&s);
        assert_eq!(s.claims.get(), 0);
        s.owned.set(false);
        check(&s);
        assert_eq!(s.claims.get(), 1);
    }
}
