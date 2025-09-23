using namespace metal;

// Minimal, not constant-time; tuned for readability first.
// Each thread takes a starting nonce and stride and computes double-SHA256,
// comparing against a target (little-endian) to mark a found result.

struct GpuParams {
    uint  midstate[8];      // SHA-256 state after first 64 bytes
    uint  block1_template[16]; // 64-byte block as 16 u32 words; time and nonce fields will be updated per-thread
    uint  target_le[8];     // target words, little-endian (u32)
    uint  time;             // header time
    uint  start_nonce;      // thread 0 nonce base
    uint  stride;           // stride between consecutive nonces per-threadgroup
    uint  count;            // number of nonces per thread
};

inline uint rotr32(uint x, uint n) { return (x >> n) | (x << (32 - n)); }
inline uint ch(uint x, uint y, uint z) { return (x & y) ^ (~x & z); }
inline uint maj(uint x, uint y, uint z) { return (x & y) ^ (x & z) ^ (y & z); }
inline uint bsig0(uint x) { return rotr32(x, 2) ^ rotr32(x, 13) ^ rotr32(x, 22); }
inline uint bsig1(uint x) { return rotr32(x, 6) ^ rotr32(x, 11) ^ rotr32(x, 25); }
inline uint ssig0(uint x) { return rotr32(x, 7) ^ rotr32(x, 18) ^ (x >> 3); }
inline uint ssig1(uint x) { return rotr32(x, 17) ^ rotr32(x, 19) ^ (x >> 10); }

constant uint K[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

inline void sha256_compress(uint state[8], thread uint w[64]) {
    uint a=state[0], b=state[1], c=state[2], d=state[3], e=state[4], f=state[5], g=state[6], h=state[7];
    for (uint t=0; t<64; ++t) {
        uint T1 = h + bsig1(e) + ch(e,f,g) + K[t] + w[t];
        uint T2 = bsig0(a) + maj(a,b,c);
        h=g; g=f; f=e; e=d + T1; d=c; c=b; b=a; a=T1 + T2;
    }
    state[0]+=a; state[1]+=b; state[2]+=c; state[3]+=d; state[4]+=e; state[5]+=f; state[6]+=g; state[7]+=h;
}

kernel void sha256d_scan(device const GpuParams*        params  [[ buffer(0) ]],
                         device atomic_uint*            found   [[ buffer(1) ]],
                         device uint2*                  result  [[ buffer(2) ]],
                         uint3                          tid     [[ thread_position_in_grid ]]) {
    const uint idx = tid.x;
    const uint count = params->count;
    const uint stride = params->stride;
    const uint base = params->start_nonce + idx * stride * count;

    for (uint i = 0; i < count; ++i) {
        uint nonce = base + i * stride;

        // First SHA256: use provided midstate and 64-byte block with time/nonce updated
        uint w[64];
        // Prepare message schedule for chunk1 (16 words) from template
    for (uint t=0; t<16; ++t) { w[t] = params->block1_template[t]; }
    // SHA-256 operates over big-endian words. Our template is BE; ensure time/nonce are written as BE words.
    uint t_be = ((params->time & 0x000000FF) << 24) |
            ((params->time & 0x0000FF00) << 8)  |
            ((params->time & 0x00FF0000) >> 8)  |
            ((params->time & 0xFF000000) >> 24);
    uint n_be = ((nonce & 0x000000FF) << 24) |
            ((nonce & 0x0000FF00) << 8)  |
            ((nonce & 0x00FF0000) >> 8)  |
            ((nonce & 0xFF000000) >> 24);
    w[1] = t_be;      // offset 4..8 (time)
    w[3] = n_be;      // offset 12..16 (nonce)
        for (uint t=16; t<64; ++t) { w[t] = ssig1(w[t-2]) + w[t-7] + ssig0(w[t-15]) + w[t-16]; }

        uint st1[8];
        for (uint j=0;j<8;++j) st1[j] = params->midstate[j];
        sha256_compress(st1, w);

        // Second SHA256: build block [digest(32)] + [0x80] + zeros + [len=256]
        uint w2[64];
        // st1 words are big-endian words already per spec; reuse directly as 8 words (which is 32 bytes)
        for (uint t=0; t<8; ++t) w2[t] = st1[t];
        w2[8] = 0x80000000; // 0x80 then zeros
        for (uint t=9; t<15; ++t) w2[t] = 0;
        w2[15] = 256; // length in bits
        for (uint t=16; t<64; ++t) { w2[t] = ssig1(w2[t-2]) + w2[t-7] + ssig0(w2[t-15]) + w2[t-16]; }

        uint st2[8] = { 0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19 };
        sha256_compress(st2, w2);

        // Compare little-endian 256-bit hash (Bitcoin-style); st2 words are big-endian, so reverse bytes here
        // We compare in u32 LE words: reverse each word and then compare from high to low (7..0)
        bool below = false;
        bool equal = true;
        for (int k = 7; k >= 0; --k) {
            // Manual byte swap (bswap32)
            uint v = st2[k];
            uint hw = ((v & 0x000000FF) << 24) |
                      ((v & 0x0000FF00) << 8)  |
                      ((v & 0x00FF0000) >> 8)  |
                      ((v & 0xFF000000) >> 24);
            uint tw = params->target_le[k];
            if (hw < tw) { below = true; equal = false; break; }
            if (hw > tw) { below = false; equal = false; break; }
        }
        if (below || equal) {
            if (atomic_exchange_explicit(found, 1u, memory_order_relaxed) == 0u) {
                // store nonce and thread index
                result[0] = uint2(nonce, idx);
            }
            return;
        }
        if (atomic_load_explicit(found, memory_order_relaxed) != 0u) return;
    }
}
