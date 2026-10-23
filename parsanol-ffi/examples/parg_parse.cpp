// Working C++ consumer of the parsanol PARG artifact ABI — the C header
// is C++-safe (`extern "C"`), and RAII handles the owned strings.
//
// Build (after `cargo build -p parsanol-ffi --release`):
//   c++ -std=c++17 examples/parg_parse.cpp -I include \
//      -o parg_parse_cpp -L ../target/release -lparsanol
//   LD_LIBRARY_PATH=../target/release ./parg_parse_cpp demo.artifact.json "iso 12345"

#include <parsanol.h>

#include <cstdio>
#include <fstream>
#include <memory>
#include <sstream>
#include <string>

namespace {

struct CStringDeleter {
    void operator()(char *ptr) const noexcept {
        if (ptr) parsanol_parg_free(ptr);
    }
};
using OwnedString = std::unique_ptr<char, CStringDeleter>;

std::string read_file(const char *path) {
    std::ifstream file(path, std::ios::binary);
    if (!file) return {};
    std::ostringstream buffer;
    buffer << file.rdbuf();
    return buffer.str();
}

}  // namespace

int main(int argc, char **argv) {
    if (argc != 3) {
        std::fprintf(stderr, "usage: %s <artifact.json> <input>\n", argv[0]);
        return 2;
    }
    std::printf("parsanol engine version: %s\n", parsanol_version());

    const std::string envelope = read_file(argv[1]);
    if (envelope.empty()) {
        std::fprintf(stderr, "cannot read %s\n", argv[1]);
        return 2;
    }

    OwnedString shaped(parsanol_parg_parse(envelope.c_str(), argv[2]));
    if (shaped) {
        std::printf("tree: %s\n", shaped.get());
    } else {
        OwnedString wire(parsanol_parg_error(envelope.c_str(), argv[2]));
        std::printf("rejected: %s\n", wire ? wire.get() : parsanol_c_last_error());
    }

    OwnedString recovered(
        parsanol_parg_parse_recovering(envelope.c_str(), nullptr, argv[2]));
    if (recovered) {
        std::printf("recovery: %s\n", recovered.get());
    }
    return shaped ? 0 : 1;
}
