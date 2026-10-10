ENTRY(_start)
SECTIONS
{
	. = 0x80010000;
	.text : { *(.text.start) *(.text*) *(.rodata*) *(.data*) *(.sdata*) *(.bss*) *(.sbss*) }
	/DISCARD/ : { *(.MIPS.*) *(.reginfo) *(.comment) *(.pdr) *(.gnu.*) *(.note*) }
}
