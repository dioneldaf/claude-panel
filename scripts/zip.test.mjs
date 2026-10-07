import { crc32, inflateRawSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { createZip } from "./zip.mjs";

/** Reads an archive back through its central directory, as an unzip tool would. */
function readZip(zip) {
  const end = zip.length - 22;
  expect(zip.readUInt32LE(end)).toBe(0x06054b50);
  const count = zip.readUInt16LE(end + 10);
  let pos = zip.readUInt32LE(end + 16);
  expect(pos + zip.readUInt32LE(end + 12)).toBe(end);
  const files = [];
  for (let i = 0; i < count; i++) {
    expect(zip.readUInt32LE(pos)).toBe(0x02014b50);
    const crc = zip.readUInt32LE(pos + 16);
    const compressedSize = zip.readUInt32LE(pos + 20);
    const size = zip.readUInt32LE(pos + 24);
    const nameLength = zip.readUInt16LE(pos + 28);
    const local = zip.readUInt32LE(pos + 42);
    const name = zip.toString("utf8", pos + 46, pos + 46 + nameLength);
    expect(zip.readUInt32LE(local)).toBe(0x04034b50);
    const dataStart = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const data = inflateRawSync(zip.subarray(dataStart, dataStart + compressedSize));
    expect(data.length).toBe(size);
    expect(crc32(data) >>> 0).toBe(crc);
    files.push({ name, data });
    pos += 46 + nameLength;
  }
  return files;
}

describe("createZip", () => {
  it("round-trips names and contents, including binary and empty files", () => {
    const entries = [
      { name: "app.exe", data: Buffer.from([0, 255, 1, 254, 77, 90]) },
      { name: "docs/README.txt", data: Buffer.from("hello\r\n".repeat(500)) },
      { name: "LICENSE", data: Buffer.alloc(0) },
    ];
    const files = readZip(createZip(entries));
    expect(files.map((f) => f.name)).toEqual(entries.map((e) => e.name));
    files.forEach((file, i) => expect(file.data.equals(entries[i].data)).toBe(true));
  });

  it("compresses and is reproducible for the same input", () => {
    const entries = [{ name: "a.txt", data: Buffer.from("a".repeat(10_000)) }];
    const zip = createZip(entries);
    expect(zip.length).toBeLessThan(500);
    expect(createZip(entries).equals(zip)).toBe(true);
  });

  it("rejects names that could escape the extraction folder", () => {
    for (const name of ["", "/abs.txt", "..\\up.txt", "a/../../b.txt"]) {
      expect(() => createZip([{ name, data: Buffer.alloc(1) }]), name).toThrow();
    }
  });
});
