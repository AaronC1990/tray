/*
 * tray's PS1 test card: a bare-metal program that drives the GPU directly,
 * so the frontend can be tested without any commercial software.
 *
 * 320x240, 15-bit colour. Eight colour bars, a one-pixel checkerboard to
 * catch uneven scaling, a white border on the outermost pixels, and a box
 * that moves one pixel per frame to prove frames advance.
 */
#include <stdint.h>

#define GP0 (*(volatile uint32_t *)0xBF801810)
#define GP1 (*(volatile uint32_t *)0xBF801814)
#define GPUSTAT (*(volatile uint32_t *)0xBF801814)
#define I_STAT (*(volatile uint32_t *)0xBF801070)

#define W 320
#define H 240

static void gp0(uint32_t word)
{
	while (!(GPUSTAT & (1u << 26)))
		;
	GP0 = word;
}

static uint32_t rgb(uint32_t r, uint32_t g, uint32_t b)
{
	return (b << 16) | (g << 8) | r;
}

/* Fill rect: x and w are rounded to 16 by the hardware, so keep them so. */
static void fill(int x, int y, int w, int h, uint32_t colour)
{
	gp0(0x02000000 | colour);
	gp0(((uint32_t)y << 16) | (uint32_t)x);
	gp0(((uint32_t)h << 16) | (uint32_t)w);
}

/* Monochrome rect at any position, drawn through the drawing area. */
static void rect(int x, int y, int w, int h, uint32_t colour)
{
	gp0(0x60000000 | colour);
	gp0(((uint32_t)y << 16) | (uint32_t)x);
	gp0(((uint32_t)h << 16) | (uint32_t)w);
}

static void wait_vblank(void)
{
	while (!(I_STAT & 1u))
		;
	I_STAT = ~1u;
}

static const uint32_t BARS[8][3] = {
	{255, 255, 255}, {255, 255, 0}, {0, 255, 255}, {0, 255, 0},
	{255, 0, 255},   {255, 0, 0},   {0, 0, 255},   {16, 16, 16},
};

static void draw(int frame)
{
	fill(0, 0, W, H, rgb(24, 24, 40));
	for (int i = 0; i < 8; i++)
		rect(i * 40, 16, 40, 96, rgb(BARS[i][0], BARS[i][1], BARS[i][2]));
	for (int y = 0; y < 48; y++)
		for (int x = (y & 1); x < 96; x += 2)
			rect(24 + x, 136 + y, 1, 1, rgb(255, 255, 255));
	rect(0, 0, W, 1, rgb(255, 255, 255));
	rect(0, H - 1, W, 1, rgb(255, 255, 255));
	rect(0, 0, 1, H, rgb(255, 255, 255));
	rect(W - 1, 0, 1, H, rgb(255, 255, 255));
	int x = 140 + (frame % 160);
	rect(x, 150, 16, 16, rgb(255, 64, 32));
}

void main(void)
{
	GP1 = 0x00000000;                         /* reset */
	GP1 = 0x08000001;                         /* 320x240, NTSC, 15-bit */
	GP1 = 0x06C60260;                         /* horizontal display range */
	GP1 = 0x07040010;                         /* vertical display range */
	GP1 = 0x05000000;                         /* display starts at VRAM 0,0 */
	gp0(0xE1000400);                          /* may draw to the displayed area */
	gp0(0xE3000000);                          /* drawing area from 0,0 */
	gp0(0xE4000000 | ((H - 1) << 10) | (W - 1)); /* to 319,239 */
	gp0(0xE5000000);                          /* no drawing offset */
	GP1 = 0x03000000;                         /* display on */

	for (int frame = 0;; frame++) {
		draw(frame);
		wait_vblank();
	}
}
