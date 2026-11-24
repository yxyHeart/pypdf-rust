/*
 * P0 probe: does the zlib implementation Pillow bundles reproduce Pillow's PNG
 * IDAT payload byte-for-byte?
 *
 * Usage: zlib_probe <in.bin> <level> <window_bits> <mem_level> <strategy> <out.bin>
 *                  [chunk_size]
 *
 * With ``chunk_size`` the input is fed in fixed size pieces with Z_NO_FLUSH,
 * mirroring Pillow's row-by-row encoding loop.
 *
 * Pictured against ``png_filtered.bin`` (the decompressed IDAT) from
 * tools/probe/image_parity.py.  PNG IDAT normally uses window_bits=15 (zlib
 * wrapper); -15 means raw deflate.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <zlib.h>

static unsigned char *read_file(const char *path, size_t *size) {
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        fprintf(stderr, "zlib_probe: cannot open %s\n", path);
        exit(1);
    }
    fseek(file, 0, SEEK_END);
    const long length = ftell(file);
    fseek(file, 0, SEEK_SET);
    unsigned char *buffer = malloc((size_t)length + 1);
    if (buffer == NULL) {
        exit(1);
    }
    if (fread(buffer, 1, (size_t)length, file) != (size_t)length) {
        exit(1);
    }
    fclose(file);
    *size = (size_t)length;
    return buffer;
}

int main(int argc, char **argv) {
    if (argc != 7 && argc != 8) {
        fprintf(stderr, "usage: zlib_probe <in.bin> <level> <wbits> <memlevel> "
                        "<strategy> <out.bin> [chunk]\n");
        return 1;
    }
    size_t input_size = 0;
    unsigned char *input = read_file(argv[1], &input_size);
    const int level = atoi(argv[2]);
    const int window_bits = atoi(argv[3]);
    const int mem_level = atoi(argv[4]);
    const int strategy = atoi(argv[5]);

    z_stream stream;
    memset(&stream, 0, sizeof(stream));
    if (deflateInit2(&stream, level, Z_DEFLATED, window_bits, mem_level, strategy) != Z_OK) {
        fprintf(stderr, "zlib_probe: deflateInit2 failed\n");
        return 1;
    }
    const size_t bound = deflateBound(&stream, (uLong)input_size) + 1024;
    unsigned char *output = malloc(bound);
    stream.next_out = output;
    stream.avail_out = (uInt)bound;
    const size_t chunk = (argc == 8) ? (size_t)atol(argv[7]) : input_size;
    size_t offset = 0;
    while (offset < input_size) {
        size_t take = input_size - offset < chunk ? input_size - offset : chunk;
        stream.next_in = input + offset;
        stream.avail_in = (uInt)take;
        const int rc = deflate(&stream, Z_NO_FLUSH);
        if (rc != Z_OK) {
            fprintf(stderr, "zlib_probe: deflate failed (%d)\n", rc);
            return 1;
        }
        offset += take;
    }
    int result;
    do {
        result = deflate(&stream, Z_FINISH);
    } while (result == Z_OK);
    if (result != Z_STREAM_END) {
        fprintf(stderr, "zlib_probe: deflate failed (%d)\n", result);
        return 1;
    }
    const size_t output_size = stream.total_out;
    deflateEnd(&stream);

    FILE *file = fopen(argv[6], "wb");
    if (file == NULL) {
        return 1;
    }
    fwrite(output, 1, output_size, file);
    fclose(file);

    printf("zlib: %s\n", zlibVersion());
    free(input);
    free(output);
    return 0;
}
