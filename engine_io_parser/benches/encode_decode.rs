use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use engine_io_parser::binary::decoder as binary_decoder;
use engine_io_parser::binary::encoder as binary_encoder;
use engine_io_parser::packet;
use engine_io_parser::string::decoder as string_decoder;
use engine_io_parser::string::encoder as string_encoder;

use packet::Packet;
use packet::PacketType;

fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("benchmarks");
    // To get the result in operations per second, just like the benchmarks
    // in the nodejs engine.io-parser library do.
    group.throughput(Throughput::Elements(1 as u64));

    group.bench_function("encode packet as string", |b| {
        b.iter(|| {
            string_encoder::encode_packet(Packet {
                packet_type: PacketType::Message,
                data: "test",
            })
        })
    });

    group.bench_function("encode packet as binary", |b| {
        b.iter(|| {
            binary_encoder::encode_packet(Packet {
                packet_type: PacketType::Message,
                data: &[1, 2, 3, 4],
            })
        })
    });

    group.bench_function("encode payload as string to string", |b| {
        b.iter(|| {
            string_encoder::encode_payload(&[
                Packet {
                    packet_type: PacketType::Message,
                    data: string_encoder::PacketData::PlaintextData("test1"),
                },
                Packet {
                    packet_type: PacketType::Message,
                    data: string_encoder::PacketData::PlaintextData("test2"),
                },
            ])
        })
    });

    group.bench_function("encode payload as string to binary", |b| {
        b.iter(|| {
            binary_encoder::encode_payload(&[
                Packet {
                    packet_type: PacketType::Message,
                    data: binary_encoder::PacketData::PlaintextData("test1"),
                },
                Packet {
                    packet_type: PacketType::Message,
                    data: binary_encoder::PacketData::PlaintextData("test2"),
                },
            ])
        })
    });

    group.bench_function("encode payload as binary", |b| {
        b.iter(|| {
            binary_encoder::encode_payload(&[
                Packet {
                    packet_type: PacketType::Message,
                    data: binary_encoder::PacketData::PlaintextData("test"),
                },
                Packet {
                    packet_type: PacketType::Message,
                    data: binary_encoder::PacketData::BinaryData(&[1, 2, 3, 4]),
                },
            ])
        })
    });

    group.bench_function("encode payload as binary to string (base64)", |b| {
        b.iter(|| {
            string_encoder::encode_payload(&[
                Packet {
                    packet_type: PacketType::Message,
                    data: string_encoder::PacketData::PlaintextData("test"),
                },
                Packet {
                    packet_type: PacketType::Message,
                    data: string_encoder::PacketData::BinaryData(&[1, 2, 3, 4]),
                },
            ])
        })
    });

    group.bench_function("decode packet from string", |b| {
        b.iter(|| string_decoder::decode_packet("4test"))
    });

    group.bench_function("decode packet from binary", |b| {
        b.iter(|| binary_decoder::decode_packet(&[4, 1, 2, 3, 4]))
    });

    group.bench_function("decode payload from string", |b| {
        b.iter(|| string_decoder::decode_payload("6:4test16:4test2"))
    });

    group.bench_function("decode payload from binary", |b| {
        b.iter(|| {
            binary_decoder::decode_payload(&[
                0x00, 0x05, 0xff, 0x34, 0x74, 0x65, 0x73, 0x74, 0x01, 0x05, 0xff, 0x04, 0x01, 0x02,
                0x03, 0x04,
            ])
        })
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
