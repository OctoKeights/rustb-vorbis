/* Prints what stb_vorbis makes of Ogg Vorbis files, through every function of its pull API, in
 * the same form as examples/dump.rs does with the Rust port. compare.sh runs both and compares
 * them.
 *
 *   cc -O2 [-DFLOAT_PI] -I <folder with stb_vorbis.c> dump.c -lm -o dump
 *   ./dump <file.ogg>...
 *
 * stb_vorbis makes its tables with the C library's M_PI, a double, and where <math.h> has none
 * (Microsoft's compiler) with a float of its own. This builds it one way or, with FLOAT_PI,
 * the other, whatever the C library here has: the port's Pi::Double and Pi::Float. With
 * FLOAT_PI and built as C++ (c++ -x c++), it is the port's Pi::FloatCpp.
 *
 * Don't build this with -ffast-math, or for a processor with fused multiply-add (-march=native):
 * the compiler then rounds differently from the port.
 */
#include <math.h>
#undef M_PI
#ifdef FLOAT_PI
#define M_PI 3.14159265358979323846264f
#else
#define M_PI 3.14159265358979323846
#endif

#define STB_VORBIS_NO_STDIO
#define STB_VORBIS_NO_PUSHDATA_API
#include "stb_vorbis.c"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* FNV-1a, 64 bits. */
static uint64_t hash_new(void) { return 14695981039346656037ull; }

static void hash_bytes(uint64_t *hash, const void *data, size_t count)
{
   const unsigned char *bytes = (const unsigned char *) data;
   size_t i;
   for (i = 0; i < count; ++i) {
      *hash ^= bytes[i];
      *hash *= 1099511628211ull;
   }
}

/* (Little-endian machines only: the samples are summed as they lie in memory.) */
static void hash_u32(uint64_t *hash, uint32_t value) { hash_bytes(hash, &value, 4); }
static void hash_shorts(uint64_t *hash, const short *samples, size_t count) { hash_bytes(hash, samples, count * 2); }
static void hash_floats(uint64_t *hash, const float *samples, size_t count) { hash_bytes(hash, samples, count * 4); }

/* Where the decoder is and the error it holds, which this clears. */
static const char *state(stb_vorbis *v)
{
   static char text[96];
   int at = stb_vorbis_get_sample_offset(v);
   unsigned int offset = stb_vorbis_get_file_offset(v);
   snprintf(text, sizeof(text), "at %d %u error %d", at, offset, stb_vorbis_get_error(v));
   return text;
}

static short **short_buffers(int channels, int chunk)
{
   short **buffers = (short **) malloc(sizeof(short *) * channels);
   int i;
   for (i = 0; i < channels; ++i)
      buffers[i] = (short *) calloc(chunk, sizeof(short));
   return buffers;
}

static void free_buffers(void **buffers, int channels)
{
   int i;
   for (i = 0; i < channels; ++i)
      free(buffers[i]);
   free(buffers);
}

/* 16-bit samples for `channels` channels, each in its own buffer. */
static void short_planar(stb_vorbis *v, int channels, int chunk, char *text)
{
   short **buffers = short_buffers(channels, chunk);
   uint64_t hash = hash_new();
   size_t total = 0;
   int i;
   for (;;) {
      int n = stb_vorbis_get_samples_short(v, channels, buffers, chunk);
      if (n == 0) break;
      for (i = 0; i < channels; ++i)
         hash_shorts(&hash, buffers[i], n);
      total += n;
   }
   free_buffers((void **) buffers, channels);
   sprintf(text, "%zu %016llx", total, (unsigned long long) hash);
}

/* Interleaved 16-bit samples for `channels` channels, `chunk` at a time, to the end. */
static void short_interleaved(stb_vorbis *v, int channels, int chunk, char *text)
{
   short *buffer;
   uint64_t hash = hash_new();
   size_t total = 0;
   /* stb_vorbis can't interleave for one channel what has more (it asserts, or writes two
      channels into the buffer): the mono mix of its function for separate buffers is what the
      port gives there. */
   if (channels == 1 && v->channels != 1) {
      short_planar(v, channels, chunk, text);
      return;
   }
   buffer = (short *) calloc((size_t) chunk * channels, sizeof(short));
   for (;;) {
      int n = stb_vorbis_get_samples_short_interleaved(v, channels, buffer, chunk * channels);
      if (n == 0) break;
      hash_shorts(&hash, buffer, (size_t) n * channels);
      total += n;
   }
   free(buffer);
   sprintf(text, "%zu %016llx", total, (unsigned long long) hash);
}

static void float_interleaved(stb_vorbis *v, int channels, int chunk, char *text)
{
   float *buffer = (float *) calloc((size_t) chunk * channels, sizeof(float));
   uint64_t hash = hash_new();
   size_t total = 0;
   for (;;) {
      int n = stb_vorbis_get_samples_float_interleaved(v, channels, buffer, chunk * channels);
      if (n == 0) break;
      hash_floats(&hash, buffer, (size_t) n * channels);
      total += n;
   }
   free(buffer);
   sprintf(text, "%zu %016llx", total, (unsigned long long) hash);
}

static void float_planar(stb_vorbis *v, int channels, int chunk, char *text)
{
   float **buffers = (float **) malloc(sizeof(float *) * channels);
   uint64_t hash = hash_new();
   size_t total = 0;
   int i;
   for (i = 0; i < channels; ++i)
      buffers[i] = (float *) calloc(chunk, sizeof(float));
   for (;;) {
      int n = stb_vorbis_get_samples_float(v, channels, buffers, chunk);
      if (n == 0) break;
      for (i = 0; i < channels; ++i)
         hash_floats(&hash, buffers[i], n);
      total += n;
   }
   free_buffers((void **) buffers, channels);
   sprintf(text, "%zu %016llx", total, (unsigned long long) hash);
}

/* Frame by frame as floats: how many frames, the samples in all, and a sum of each frame's
   place, length and samples. A frame without samples does not end it; ten in a row do. */
static void frames_float(stb_vorbis *v, int channels, char *text)
{
   uint64_t hash = hash_new();
   size_t frames = 0, total = 0;
   int empty = 0, i;
   while (empty < 10) {
      float **output = NULL;
      int n = stb_vorbis_get_frame_float(v, NULL, &output);
      hash_u32(&hash, (uint32_t) stb_vorbis_get_sample_offset(v));
      hash_u32(&hash, (uint32_t) n);
      if (n == 0) {
         ++empty;
         continue;
      }
      empty = 0;
      for (i = 0; i < channels; ++i)
         hash_floats(&hash, output[i], n);
      ++frames;
      total += n;
   }
   sprintf(text, "%zu %zu %016llx", frames, total, (unsigned long long) hash);
}

/* Frame by frame as interleaved 16-bit samples, with room for `room` samples. */
static void frames_short_interleaved(stb_vorbis *v, int channels, int room, char *text)
{
   short *buffer = (short *) calloc(room, sizeof(short));
   uint64_t hash = hash_new();
   size_t frames = 0, total = 0;
   for (;;) {
      int n = stb_vorbis_get_frame_short_interleaved(v, channels, buffer, room);
      if (n == 0) break;
      hash_shorts(&hash, buffer, (size_t) n * channels);
      ++frames;
      total += n;
   }
   free(buffer);
   sprintf(text, "%zu %zu %016llx", frames, total, (unsigned long long) hash);
}

/* Frame by frame as 16-bit samples, a buffer of `room` samples for each channel. */
static void frames_short_planar(stb_vorbis *v, int channels, int room, char *text)
{
   short **buffers = short_buffers(channels, room);
   uint64_t hash = hash_new();
   size_t frames = 0, total = 0;
   int i;
   for (;;) {
      int n = stb_vorbis_get_frame_short(v, channels, buffers, room);
      if (n == 0) break;
      for (i = 0; i < channels; ++i)
         hash_shorts(&hash, buffers[i], n);
      ++frames;
      total += n;
   }
   free_buffers((void **) buffers, channels);
   sprintf(text, "%zu %zu %016llx", frames, total, (unsigned long long) hash);
}

static void dump(const char *path)
{
   const char *name = path, *p;
   unsigned char *data = NULL;
   long size = 0;
   FILE *file;
   stb_vorbis *v;
   stb_vorbis_info info;
   stb_vorbis_comment comment;
   uint64_t hash;
   int error = 0, channels, i, k;
   unsigned int length, modulus, targets[10];
   float seconds;
   char text[128], at[96];

   for (p = path; *p; ++p)
      if (*p == '/' || *p == '\\') name = p + 1;
   printf("file %s\n", name);

   file = fopen(path, "rb");
   if (file) {
      fseek(file, 0, SEEK_END);
      size = ftell(file);
      fseek(file, 0, SEEK_SET);
      data = (unsigned char *) malloc(size > 0 ? size : 1);
      if (fread(data, 1, size, file) != (size_t) size) {
         free(data);
         data = NULL;
      }
      fclose(file);
   }
   if (!data) {
      printf("unreadable\n");
      return;
   }

   v = stb_vorbis_open_memory(data, (int) size, &error, NULL);
   if (!v) {
      printf("open error %d\n", error);
      free(data);
      return;
   }
   info = stb_vorbis_get_info(v);
   channels = info.channels;
   printf("info %d %u %d\n", info.channels, info.sample_rate, info.max_frame_size);

   comment = stb_vorbis_get_comment(v);
   hash = hash_new();
   hash_bytes(&hash, comment.vendor, strlen(comment.vendor));
   for (i = 0; i < comment.comment_list_length; ++i) {
      hash_u32(&hash, (uint32_t) strlen(comment.comment_list[i]));
      hash_bytes(&hash, comment.comment_list[i], strlen(comment.comment_list[i]));
   }
   /* (A broken header can give stb_vorbis a negative number of comments: none.) */
   printf("comments %zu %d %016llx\n", strlen(comment.vendor), comment.comment_list_length > 0 ? comment.comment_list_length : 0, (unsigned long long) hash);
   printf("opened %s\n", state(v));

   length = stb_vorbis_stream_length_in_samples(v);
   seconds = stb_vorbis_stream_length_in_seconds(v);
   hash = 0;
   memcpy(&hash, &seconds, 4);
   printf("length %u %08x %s\n", length, (unsigned int) hash, state(v));

   short_interleaved(v, channels, 4096, text);
   printf("short %s %s\n", text, state(v));

   i = stb_vorbis_seek_start(v);
   printf("start %d %s\n", i, state(v));
   frames_float(v, channels, text);
   printf("frames %s %s\n", text, state(v));

   stb_vorbis_seek_start(v);
   short_interleaved(v, 1, 1000, text);
   printf("mono %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   short_planar(v, 1, 777, text);
   printf("mono planar %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   short_interleaved(v, 2, 1000, text);
   printf("stereo %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   short_planar(v, 2, 3001, text);
   printf("stereo planar %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   short_interleaved(v, 3, 500, text);
   printf("three %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   short_planar(v, 3, 64, text);
   printf("three planar %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   float_interleaved(v, 1, 1024, text);
   printf("float one %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   float_interleaved(v, 3, 333, text);
   printf("float three %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   float_planar(v, 2, 2048, text);
   printf("float planar %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   frames_short_interleaved(v, channels, 4096 * channels, text);
   printf("frames short %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   frames_short_interleaved(v, 2, 300, text);
   printf("frames short cut %s %s\n", text, state(v));
   stb_vorbis_seek_start(v);
   frames_short_planar(v, 2, 100, text);
   printf("frames planar %s %s\n", text, state(v));

   modulus = length + 1;
   if (modulus < 1) modulus = 1;
   targets[0] = 0;
   targets[1] = 1;
   targets[2] = length / 3;
   targets[3] = length / 2;
   targets[4] = length - 1;
   targets[5] = length;
   targets[6] = length + 1;
   targets[7] = length / 7 * 5;
   targets[8] = 12345 % modulus;
   targets[9] = 0;
   for (k = 0; k < 10; ++k) {
      short *buffer = (short *) calloc((size_t) 300 * channels, sizeof(short));
      float **output = NULL;
      int ok, n;

      ok = stb_vorbis_seek(v, targets[k]);
      strcpy(at, state(v));
      n = stb_vorbis_get_samples_short_interleaved(v, channels, buffer, 300 * channels);
      hash = hash_new();
      hash_shorts(&hash, buffer, (size_t) n * channels);
      printf("seek %u %d %s %d %016llx %s\n", targets[k], ok, at, n, (unsigned long long) hash, state(v));
      free(buffer);

      ok = stb_vorbis_seek_frame(v, targets[k]);
      strcpy(at, state(v));
      n = stb_vorbis_get_frame_float(v, NULL, &output);
      hash = hash_new();
      for (i = 0; n != 0 && i < channels; ++i)
         hash_floats(&hash, output[i], n);
      printf("seek frame %u %d %s %d %016llx %s\n", targets[k], ok, at, n, (unsigned long long) hash, state(v));
   }
   stb_vorbis_close(v);

   {
      short *samples = NULL;
      int rate = 0;
      int frames = stb_vorbis_decode_memory(data, (int) size, &channels, &rate, &samples);
      if (frames < 0) {
         printf("whole error\n");
      } else {
         hash = hash_new();
         hash_shorts(&hash, samples, (size_t) frames * channels);
         printf("whole %d %d %d %016llx\n", channels, rate, frames, (unsigned long long) hash);
         free(samples);
      }
   }
   free(data);
}

int main(int argc, char **argv)
{
   int i;
   for (i = 1; i < argc; ++i)
      dump(argv[i]);
   return 0;
}
