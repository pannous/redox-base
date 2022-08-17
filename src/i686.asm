BITS 64

section .text

global ustart
extern start

ustart:
  ; Setup a stack.
  mov eax, 0x21000384 ; SYS_CLASS_FILE+SYS_ARG_SLICE+SYS_MMAP
  mov ebx, 0xFFFFFFFF ; dummy `fd`, indicates anonymous map
  mov ecx, map ; pointer to Map struct
  mov edx, map_size ; size of Map struct
  int 0x80

  ; Test for success (nonzero value).
  cmp eax, 0
  jg .continue
  ; (failure)
  ud2
.continue:
  ; Subtract 16 since all instructions seem to hate non-canonical ESP values :)
  lea esp, [eax+size-16]
  mov ebp, esp

  ; Stack has the same alignment as `size`.
  call start
  ; `start` must never return.
  ud2

section .rodata
map:
  dd 0 ; offset (unused)
  dd size
  dd flags
  dd address - size

map_size equ $ - map

size equ 65536 ; 64 KiB
address equ 0x80000000 ; highest possible (normal) user address, why not
flags equ 0x0006000E ; PROT_READ+PROT_WRITE, MAP_PRIVATE+MAP_FIXED_NOREPLACE
