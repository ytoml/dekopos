; System V AMD64 Calling Convention
; Registers: RDI, RSI, RDX, RCX, R8, R9

bits 64
extern kernel_main
KERNEL_MAIN_STACK_SIZE equ 1024 * 1024

; reserving stack for kernel, avoiding continuing to use the memory
; that boot loader previously used.
section .bss align=16 
kernel_main_stack:
    resb KERNEL_MAIN_STACK_SIZE

section .text
global pre_kernel_main
pre_kernel_main:
    mov rsp, kernel_main_stack + KERNEL_MAIN_STACK_SIZE
    call kernel_main
.fin:
    hlt
    jmp .fin

