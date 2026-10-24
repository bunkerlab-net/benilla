// SPDX-License-Identifier: GPL-3.0-or-later
// Authored inputs, actual pinned libstdc++ container, no ported ordering algorithm.
#include <algorithm>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <memory>
#include <numeric>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include "vec3d.h"

// PINNED_SOURCE_TYPES

using References = decltype(WMODoodadData::References);
static_assert(std::is_same_v<References, std::unordered_set<std::uint16_t>>);
static_assert(sizeof(uint16) == 2 && sizeof(std::size_t) == 8);
static_assert(__GLIBCXX__ == 20250315 && _GLIBCXX_RELEASE == 14);
static_assert(__GNUC__ == 14 && __GNUC_MINOR__ == 2 && __GNUC_PATCHLEVEL__ == 0);
static_assert(_GLIBCXX_USE_CXX11_ABI == 1);
static_assert(std::__is_fast_hash<std::hash<uint16>>::value);
static_assert(!std::__cache_default<uint16, std::hash<uint16>>::value);
#if defined(_GLIBCXX_DEBUG) || defined(_GLIBCXX_ASSERTIONS)
#error This exact observed profile has neither debug nor assertions mode enabled.
#endif

void require(bool condition, const char* message)
{
    if (!condition)
        throw std::runtime_error(message);
}

void write_u16(std::ostream& stream, uint16 value)
{
    const char bytes[2] = {char(value & 255), char(value >> 8)};
    stream.write(bytes, 2);
}

struct Recorder
{
    std::ofstream cases, checkpoints, insertions, iterations, swaps, schedule, profile;
    std::size_t input_offset = 0, iteration_offset = 0;
    std::vector<std::size_t> buckets;
    std::vector<std::size_t> checkpoint_counts;
    std::size_t case_count = 0, checkpoint_count = 0, full_growth_checks = 0;

    explicit Recorder(const std::string& output)
        : cases(output + "/cases.tsv"), checkpoints(output + "/checkpoints.tsv"),
          insertions(output + "/insertions.u16le", std::ios::binary),
          iterations(output + "/iterations.u16le", std::ios::binary),
          swaps(output + "/swap_checks.tsv"), schedule(output + "/bucket_schedule.tsv"),
          profile(output + "/container_profile.tsv")
    {
        require(cases && checkpoints && insertions && iterations && swaps && schedule && profile,
                "Cannot open oracle outputs");
        cases << "# case\tinput_offset\tinput_count\tprepopulate_count\n";
        checkpoints << "# case\tafter_inserts\tdistinct_count\tbucket_count\titeration_offset\titeration_count\n";
        swaps << "# case\tprior_distinct\tprior_buckets\treset_distinct\treset_buckets\tdisplaced_distinct\tdisplaced_buckets\n";
        schedule << "# distinct_count\tbucket_count\n";
        profile << "# item\tvalue\n"
                << "GLIBCXX_RELEASE\t" << _GLIBCXX_RELEASE << '\n'
                << "GLIBCXX_DATE\t" << __GLIBCXX__ << '\n'
                << "GLIBCXX_USE_CXX11_ABI\t" << _GLIBCXX_USE_CXX11_ABI << '\n'
                << "GLIBCXX_DEBUG\tundefined\n"
                << "GLIBCXX_ASSERTIONS\tundefined\n"
                << "cplusplus\t" << __cplusplus << '\n'
                << "sizeof_uint16\t" << sizeof(uint16) << '\n'
                << "sizeof_size_t\t" << sizeof(std::size_t) << '\n'
                << "sizeof_pointer\t" << sizeof(void*) << '\n'
                << "sizeof_set\t" << sizeof(References) << '\n'
                << "sizeof_node\t" << sizeof(std::__detail::_Hash_node<uint16, false>) << '\n'
                << "alignof_node\t" << alignof(std::__detail::_Hash_node<uint16, false>) << '\n'
                << "sizeof_prime_policy\t" << sizeof(std::__detail::_Prime_rehash_policy) << '\n'
                << "hash_cached\t" << std::__cache_default<uint16, std::hash<uint16>>::value << '\n'
                << "small_size_threshold\t"
                << std::__detail::_Hashtable_hash_traits<std::hash<uint16>>::__small_size_threshold() << '\n'
                << "max_load_factor\t" << References().max_load_factor() << '\n'
                << "hash_identity_checked_keys\t65536\n"
                << "bucket_capacity_bound\t131072\n";
        for (std::size_t key = 0; key != 65536; ++key)
            require(std::hash<uint16>()(uint16(key)) == key, "Hash is not uint16 identity");

        // Measure every cardinality using the actual linked prime-policy implementation.
        References observed;
        buckets.push_back(observed.bucket_count());
        schedule << "0\t" << observed.bucket_count() << '\n';
        checkpoint_counts.push_back(0);
        for (std::size_t key = 0; key != 65536; ++key)
        {
            const auto previous = observed.bucket_count();
            require(observed.insert(uint16(key)).second, "Ascending measurement duplicated a key");
            buckets.push_back(observed.bucket_count());
            require(observed.bucket_count() <= 131072, "Reference buckets exceed production bound");
            if (observed.bucket_count() != previous)
            {
                schedule << key + 1 << '\t' << observed.bucket_count() << '\n';
                for (std::size_t count : {key, key + 1, key + 2})
                    if (count <= 65536)
                        checkpoint_counts.push_back(count);
            }
        }
        checkpoint_counts.push_back(65535);
        checkpoint_counts.push_back(65536);
        std::sort(checkpoint_counts.begin(), checkpoint_counts.end());
        checkpoint_counts.erase(std::unique(checkpoint_counts.begin(), checkpoint_counts.end()),
                                checkpoint_counts.end());
        profile << "maximum_observed_bucket_count\t" << observed.bucket_count() << '\n'
                << "measured_cardinalities\t" << buckets.size() << '\n';
    }

    void snapshot(const std::string& name, std::size_t count, const WMODoodadData& doodadData)
    {
        const auto& references = doodadData.References;
        checkpoints << name << '\t' << count << '\t' << references.size() << '\t'
                    << references.bucket_count() << '\t' << iteration_offset << '\t'
                    << references.size() << '\n';
        // PINNED_SOURCE_ITERATION
            write_u16(iterations, doodadIndex);
        iteration_offset += references.size();
        ++checkpoint_count;
    }

    void run(const std::string& name, const std::vector<uint16>& input,
             std::size_t prepopulate_count = 0, bool full_permutation = false,
             std::size_t duplicate_at_capacity = 0)
    {
        require(prepopulate_count <= 65536, "Invalid authored prepopulation");
        std::unordered_map<std::string, WMODoodadData> WmoDoodads;
        const std::string plain_name = "authored.wmo";
        auto& old = WmoDoodads[plain_name];
        for (std::size_t value = 0; value != prepopulate_count; ++value)
            old.References.insert(uint16(value));
        const auto prior_buckets = old.References.bucket_count();
        struct AuthoredRoot { WMODoodadData DoodadData; } froot;
        // PINNED_SOURCE_SWAP
        auto& references = doodads.References;
        require(references.empty() && references.bucket_count() == buckets[0],
                "Composite empty-swap retained prior state or buckets");
        require(froot.DoodadData.References.size() == prepopulate_count &&
                froot.DoodadData.References.bucket_count() == prior_buckets,
                "Composite empty-swap did not displace old collection");
        require(references.max_load_factor() == 1.0f, "Unexpected default maximum load");
        swaps << name << '\t' << prepopulate_count << '\t' << prior_buckets << '\t'
              << references.size() << '\t' << references.bucket_count() << '\t'
              << froot.DoodadData.References.size() << '\t'
              << froot.DoodadData.References.bucket_count() << '\n';
        cases << name << '\t' << input_offset << '\t' << input.size() << '\t'
              << prepopulate_count << '\n';
        for (uint16 value : input)
            write_u16(insertions, value);
        input_offset += input.size();
        snapshot(name, 0, doodads);
        std::vector<uint16> before_duplicates;
        for (std::size_t index = 0; index != input.size(); ++index)
        {
            const auto previous_size = references.size();
            const auto previous_buckets = references.bucket_count();
            const uint16 groupReference = input[index];
            // PINNED_SOURCE_INSERT
            require(references.bucket_count() == buckets[references.size()],
                    "Growth schedule differs at the same cardinality");
            if (references.size() == previous_size)
                require(references.bucket_count() == previous_buckets,
                        "Duplicate insertion changed bucket count");
            const std::size_t count = index + 1;
            if (full_permutation)
            {
                if (count <= 65536)
                {
                    require(references.size() == count, "Full input is not a permutation");
                    ++full_growth_checks;
                }
                else
                    require(references.size() == 65536, "Duplicate pass changed cardinality");
                if (count == 65536)
                    before_duplicates.assign(references.begin(), references.end());
            }
            const bool bounded_small = input.size() <= 300;
            if (duplicate_at_capacity ? count >= duplicate_at_capacity :
                (bounded_small || count == input.size() ||
                 (count <= 65536 && std::binary_search(checkpoint_counts.begin(), checkpoint_counts.end(), count))))
                snapshot(name, count, doodads);
        }
        if (full_permutation)
        {
            require(input.size() == 131072, "Full permutation lacks complete duplicate pass");
            require(std::equal(references.begin(), references.end(), before_duplicates.begin(),
                               before_duplicates.end()), "Duplicate pass reordered iteration");
        }
        ++case_count;
    }

    void finish()
    {
        profile << "cases\t" << case_count << '\n'
                << "checkpoints\t" << checkpoint_count << '\n'
                << "full_permutation_growth_checks\t" << full_growth_checks << '\n'
                << "literal_input_u16_count\t" << input_offset << '\n'
                << "literal_iteration_u16_count\t" << iteration_offset << '\n';
        for (auto* stream : {&cases, &checkpoints, &insertions, &iterations, &swaps, &schedule, &profile})
        {
            stream->flush();
            require(bool(*stream), "Failed writing oracle outputs");
        }
    }
};

int main(int argc, char** argv)
{
    try
    {
        require(argc == 2, "Usage: oracle OUTPUT_DIRECTORY");
        Recorder recorder(argv[1]);
        recorder.run("empty", {});
        recorder.run("singleton_zero", {0});
        recorder.run("singleton_max", {65535});
        recorder.run("duplicates", {7, 7, 0, 7, 0, 65535, 65535, 1, 1, 7});
        recorder.run("mixed_bucket_fronts", {0, 13, 1, 14, 26, 27, 2, 15, 39, 40, 28, 3, 52, 29, 58, 87, 116});
        for (uint32 stride : {13, 29, 59, 127, 257})
        {
            std::vector<uint16> values;
            for (uint32 key = 0; key <= 65535 && values.size() < 260; key += stride)
                values.push_back(uint16(key));
            recorder.run("collision_stride_" + std::to_string(stride), values);
            std::reverse(values.begin(), values.end());
            recorder.run("collision_reverse_" + std::to_string(stride), values);
        }
        std::vector<uint16> small = {0, 1, 13, 26, 29, 65535};
        std::size_t permutation = 0;
        do
            recorder.run("perm6_" + std::to_string(permutation++), small);
        while (std::next_permutation(small.begin(), small.end()));
        require(permutation == 720, "Incomplete exhaustive six-key permutations");

        // A duplicate at the old load threshold must not advance the prime policy.
        for (std::size_t capacity = 1; capacity < 65536; ++capacity)
        {
            if (recorder.buckets[capacity] == recorder.buckets[capacity + 1])
                continue;
            std::vector<uint16> values(capacity);
            std::iota(values.begin(), values.end(), uint16(0));
            values.push_back(0);
            values.push_back(uint16(capacity));
            recorder.run("duplicate_at_capacity_" + std::to_string(capacity), values, 0, false, capacity);
        }

        recorder.run("populated_root_replacement_empty", {}, 65536);
        recorder.run("populated_root_replacement_small", {65535, 0, 13, 26, 1, 65535, 29}, 65536);
        for (const std::string kind : {"ascending", "descending", "affine", "shuffled"})
        {
            std::vector<uint16> values(65536);
            std::iota(values.begin(), values.end(), uint16(0));
            if (kind == "descending")
                std::reverse(values.begin(), values.end());
            if (kind == "affine")
                for (std::size_t index = 0; index != 65536; ++index)
                    values[index] = uint16(index * 65521u + 12345u);
            if (kind == "shuffled")
            {
                uint32 state = 0x6d2b79f5;
                for (std::size_t remaining = values.size(); remaining > 1; --remaining)
                {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    std::swap(values[remaining - 1], values[state % remaining]);
                }
            }
            // A complete reverse-order duplicate pass, without touching container capacity.
            values.reserve(131072);
            for (std::size_t index = 65536; index != 0; --index)
                values.push_back(values[index - 1]);
            recorder.run("full_" + kind, values, kind == "affine" ? 65536 : 0, true);
        }
        recorder.finish();
    }
    catch (const std::exception& error)
    {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
