BITS 64

section .text

global ustart
extern start

ustart:
  ; Setup a stack.
  mov rax, 0x21000384 ; SYS_CLASS_FILE+SYS_ARG_SLICE+SYS_MMAP
  mov rdi, 0xFFFFFFFFFFFFFFFF ; dummy `fd`, indicates anonymous map
  mov rsi, map ; pointer to Map struct
  mov rdx, map_size ; size of Map struct
  syscall

  ; Test for success (nonzero value).
  cmp rax, 0
  jg .continue
  ; (failure)
  ud2
.continue:
  ; Subtract 16 since all instructions seem to hate non-canonical RSP values :)
  lea rsp, [rax+size-16]
  mov rbp, rsp

  ; Stack has the same alignment as `size`.
  call start
  ; `start` must never return.
  ud2

section .rodata
map:
  dq 0 ; offset (unused)
  dq size
  dq flags
  dq address - size

map_size equ $ - map

size equ 65536 ; 64 KiB
address equ 0x0000800000000000 ; highest possible (normal) user address, why not
flags equ 0x0006000E ; PROT_READ+PROT_WRITE, MAP_PRIVATE+MAP_FIXED_NOREPLACE
