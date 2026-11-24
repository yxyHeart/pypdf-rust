/*
 * P0 probe: does libjpeg-turbo reproduce Pillow's default JPEG bytes?
 *
 * Usage: jpeg_probe <in.rgb> <width> <height> <quality> <out.jpg>
 *
 * Mirrors what Pillow's JpegImagePlugin._save does with default encoderinfo:
 *   - JCS_RGB input, 3 components
 *   - jpeg_set_defaults()
 *   - jpeg_set_quality(quality, force_baseline = TRUE)
 *   - no optimize_coding, no progressive, sampling factors left at defaults
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <jpeglib.h>

static void fail(const char *message) {
    fprintf(stderr, "jpeg_probe: %s\n", message);
    exit(1);
}

int main(int argc, char **argv) {
    if (argc != 6) {
        fail("usage: jpeg_probe <in.rgb> <width> <height> <quality> <out.jpg>");
    }
    const char *input_path = argv[1];
    const int width = atoi(argv[2]);
    const int height = atoi(argv[3]);
    const int quality = atoi(argv[4]);
    const char *output_path = argv[5];

    const size_t expected = (size_t)width * (size_t)height * 3;
    unsigned char *pixels = malloc(expected);
    if (pixels == NULL) {
        fail("out of memory");
    }
    FILE *input = fopen(input_path, "rb");
    if (input == NULL) {
        fail("cannot open input");
    }
    if (fread(pixels, 1, expected, input) != expected) {
        fail("short read");
    }
    fclose(input);

    struct jpeg_compress_struct cinfo;
    struct jpeg_error_mgr jerr;
    cinfo.err = jpeg_std_error(&jerr);
    jpeg_create_compress(&cinfo);

    FILE *output = fopen(output_path, "wb");
    if (output == NULL) {
        fail("cannot open output");
    }
    jpeg_stdio_dest(&cinfo, output);

    cinfo.image_width = width;
    cinfo.image_height = height;
    cinfo.input_components = 3;
    cinfo.in_color_space = JCS_RGB;
    jpeg_set_defaults(&cinfo);
    jpeg_set_quality(&cinfo, quality, TRUE);

    jpeg_start_compress(&cinfo, TRUE);
    while (cinfo.next_scanline < cinfo.image_height) {
        JSAMPROW row = pixels + (size_t)cinfo.next_scanline * width * 3;
        jpeg_write_scanlines(&cinfo, &row, 1);
    }
    jpeg_finish_compress(&cinfo);
    fclose(output);
    jpeg_destroy_compress(&cinfo);
    free(pixels);

#ifdef LIBJPEG_TURBO_VERSION
#define PROBE_STR2(x) #x
#define PROBE_STR(x) PROBE_STR2(x)
    printf("libjpeg-turbo: %s (API %d)\n", PROBE_STR(LIBJPEG_TURBO_VERSION), JPEG_LIB_VERSION);
#else
    printf("libjpeg API: %d\n", JPEG_LIB_VERSION);
#endif
    return 0;
}
