/* Times stb_vorbis the way examples/bench.rs times the Rust port: each file is opened and
 * decoded to the end `repeats` times, as interleaved 16-bit samples and frame by frame as
 * floats from memory, and as interleaved 16-bit samples read from disk as they are needed.
 *
 *   cc -O2 -I <folder with stb_vorbis.c> bench.c -lm -o bench
 *   ./bench <repeats> <file.ogg>...
 */
#define _POSIX_C_SOURCE 199309L
#include "stb_vorbis.c"

#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now(void)
{
   struct timespec t;
   clock_gettime(CLOCK_MONOTONIC, &t);
   return (double) t.tv_sec + (double) t.tv_nsec * 1e-9;
}

int main(int argc, char **argv)
{
   int repeats = argc > 1 ? atoi(argv[1]) : 1;
   double shorts = 0, floats = 0, disk = 0, seconds = 0;
   unsigned long long samples = 0;
   int i, r;

   for (i = 2; i < argc; ++i) {
      FILE *file = fopen(argv[i], "rb");
      unsigned char *data;
      long size;
      double start;
      if (!file) continue;
      fseek(file, 0, SEEK_END);
      size = ftell(file);
      fseek(file, 0, SEEK_SET);
      data = (unsigned char *) malloc(size > 0 ? size : 1);
      if (fread(data, 1, size, file) != (size_t) size) size = 0;
      fclose(file);

      start = now();
      for (r = 0; r < repeats; ++r) {
         int error = 0;
         stb_vorbis *v = stb_vorbis_open_memory(data, (int) size, &error, NULL);
         stb_vorbis_info info;
         short *buffer;
         if (!v) break;
         info = stb_vorbis_get_info(v);
         buffer = (short *) malloc(sizeof(short) * 4096 * info.channels);
         for (;;) {
            int n = stb_vorbis_get_samples_short_interleaved(v, info.channels, buffer, 4096 * info.channels);
            if (n == 0) break;
            samples += n;
            if (r == 0) seconds += (double) n / info.sample_rate;
         }
         free(buffer);
         stb_vorbis_close(v);
      }
      shorts += now() - start;

      start = now();
      for (r = 0; r < repeats; ++r) {
         int error = 0, empty = 0;
         stb_vorbis *v = stb_vorbis_open_memory(data, (int) size, &error, NULL);
         if (!v) break;
         while (empty < 4) {
            float **output;
            int n = stb_vorbis_get_frame_float(v, NULL, &output);
            if (n == 0) ++empty; else empty = 0;
            samples += n;
         }
         stb_vorbis_close(v);
      }
      floats += now() - start;
      free(data);

      start = now();
      for (r = 0; r < repeats; ++r) {
         int error = 0;
         stb_vorbis *v = stb_vorbis_open_filename(argv[i], &error, NULL);
         stb_vorbis_info info;
         short *buffer;
         if (!v) break;
         info = stb_vorbis_get_info(v);
         buffer = (short *) malloc(sizeof(short) * 4096 * info.channels);
         for (;;) {
            int n = stb_vorbis_get_samples_short_interleaved(v, info.channels, buffer, 4096 * info.channels);
            if (n == 0) break;
            samples += n;
         }
         free(buffer);
         stb_vorbis_close(v);
      }
      disk += now() - start;
   }
   printf("%.1f seconds of sound, %d times each way (%llu samples)\n", seconds, repeats, samples);
   printf("16-bit samples: %.3f s (%.0f times as fast as it plays)\n", shorts, seconds * repeats / shorts);
   printf("float frames:   %.3f s (%.0f times as fast as it plays)\n", floats, seconds * repeats / floats);
   printf("16-bit, on disk: %.3f s (%.0f times as fast as it plays)\n", disk, seconds * repeats / disk);
   return 0;
}
