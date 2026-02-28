import { describe, expect, it } from "vitest";
import { clampPage, getTotalPages, paginateRows } from "./table-pagination";

describe("table-pagination", () => {
  it("computes total pages with lower bound 1", () => {
    expect(getTotalPages(0, 10)).toBe(1);
    expect(getTotalPages(21, 10)).toBe(3);
  });

  it("clamps out-of-range page", () => {
    expect(clampPage(0, 25, 10)).toBe(1);
    expect(clampPage(99, 25, 10)).toBe(3);
  });

  it("slices rows by current page and page size", () => {
    const rows = Array.from({ length: 12 }, (_, index) => index + 1);
    expect(paginateRows(rows, 1, 5)).toEqual([1, 2, 3, 4, 5]);
    expect(paginateRows(rows, 3, 5)).toEqual([11, 12]);
  });
});
