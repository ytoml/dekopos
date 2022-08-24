use heapless::spsc::Queue;
use log;

use super::usb::status::Running;
use super::usb::HostController;
use crate::x64::Msr;
use x86_64::instructions::interrupts as x64;
use x86_64::structures::idt::{Entry, HandlerFunc, InterruptDescriptorTable, InterruptStackFrame};
use x86_64::PrivilegeLevel;

/// # Safety
/// Caller must call this only once just after entering the kernel.
pub unsafe fn setup_handler() {
    setup_apic_base();
    setup_handler_inner();
    log::info!("apic base: {:?}", APIC_BASE);
}

pub fn process_interrupt_messages(ctr: &mut HostController<Running>) -> ! {
    let que = unsafe { int_que_mut() };
    log::info!("{que:?}");
    loop {
        x64::disable();
        if let Some(msg) = que.dequeue() {
            x64::enable();
            match msg {
                Message::XhciInterrupt => {
                    if let Err(e) = ctr.process_events() {
                        log::error!("HostController::process_event failed: {e:?}");
                    }
                }
            }
        } else {
            x64::enable_and_hlt();
        }
    }
}

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
static mut INT_QUE: Queue<Message, { INT_QUE_SIZE }> = Queue::new();
// access_static_as_mut_unwrap!(int_que_mut, INT_QUE, Queue<Message, { INT_QUE_SIZE}>);
access_static_mut!(
    int_que_mut,
    INT_QUE,
    Queue<Message, { INT_QUE_SIZE }>
);
access_static_mut!(
    interrupt_descriptor_table_mut,
    IDT,
    InterruptDescriptorTable
);

pub(super) const XHCI_INTVEC_ID: usize = 0x40;
const INT_QUE_SIZE: usize = 1024 * 128;
fn setup_handler_inner() {
    let mut hdl_entry = Entry::<HandlerFunc>::missing();

    // Note that EntryOption does not seems to be provide method to specify Gate type(bits 8..12),
    // but it actually set bits 9..12 by default and provide way to switch Interrupt Gate and Trap Gate
    // by toggling bit 8 through method disable_interrupts().
    hdl_entry
        .set_handler_fn(xhci)
        .set_privilege_level(PrivilegeLevel::Ring0)
        .disable_interrupts(true); // Interrupt Gate
    let idt = unsafe { interrupt_descriptor_table_mut() };
    idt[XHCI_INTVEC_ID] = hdl_entry;
    idt.load();
    log::info!("Interrupt handler set.");
    // init_int_que();
}

// /// Initialize only once. Multiple calls will be ignored.
// fn init_int_que() {
//     unsafe { int_que_option_mut() }.get_or_insert(Queue::new());
// }

#[derive(Debug)]
enum Message {
    XhciInterrupt,
}

extern "x86-interrupt" fn xhci(_frame: InterruptStackFrame) {
    // look into Event rings and process events
    log::warn!("interruptor entered.");
    if unsafe { int_que_mut() }
        .enqueue(Message::XhciInterrupt)
        .is_ok()
    {
        notify_end();
    } else {
        log::warn!("xHCI int handler: Interrupt que is full.");
    }
}

static mut APIC_BASE: Option<*mut u8> = None;
access_static_as_ref_unwrap!(apic_base, APIC_BASE, *mut u8);
access_static_mut!(apic_base_option_mut, APIC_BASE, Option<*mut u8>);

/// Returns APIC Base address + offset.
pub fn get_apic_addr(offset: usize) -> *mut u8 {
    unsafe { *apic_base() }.wrapping_add(offset)
}

fn notify_end() {
    const EOI_OFFSET: usize = 0xb0;
    let addr = get_apic_addr(EOI_OFFSET) as *mut u32;
    unsafe { addr.write_volatile(0) };
}

pub fn get_local_apic_id() -> u32 {
    const OFFSET: usize = 0x20;
    let addr = get_apic_addr(OFFSET) as *mut u32;
    // (unsafe { addr.read_volatile() } >> 24) as u8
    unsafe { addr.read_volatile() }
}

/// Initialize only once. Multiple calls will be ignored.
fn init_apic_base(base: *mut u8) {
    unsafe { apic_base_option_mut() }.get_or_insert(base);
}

const APIC_BASE_SPECIFIER: u32 = 0x1b;
const MSR: Msr = Msr::new(APIC_BASE_SPECIFIER);
fn setup_apic_base() {
    // Note: If you want to know about hard coded numbers, see https://wiki.osdev.org/APIC
    let base = unsafe { MSR.read() } & !0xfffu64; // extract bits 12..max_phy_addr
    init_apic_base(base as *mut u8);
}
