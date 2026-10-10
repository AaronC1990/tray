	.set noreorder
	.section .text.start
	.global _start
_start:
	la	$sp, 0x801FFF00
	jal	main
	nop
1:	b	1b
	nop
