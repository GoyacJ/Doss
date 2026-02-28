export function normalizePageSize(pageSize: number, fallback = 10): number {
  if (!Number.isFinite(pageSize)) {
    return fallback;
  }
  const normalized = Math.trunc(pageSize);
  return normalized > 0 ? normalized : fallback;
}

export function getTotalPages(total: number, pageSize: number): number {
  const safeTotal = Math.max(0, Math.trunc(total));
  const safePageSize = normalizePageSize(pageSize);
  return Math.max(1, Math.ceil(safeTotal / safePageSize));
}

export function clampPage(page: number, total: number, pageSize: number): number {
  const safePage = Number.isFinite(page) ? Math.trunc(page) : 1;
  const totalPages = getTotalPages(total, pageSize);
  return Math.min(Math.max(1, safePage), totalPages);
}

export function paginateRows<Row>(rows: Row[], page: number, pageSize: number): Row[] {
  const safePageSize = normalizePageSize(pageSize);
  const currentPage = clampPage(page, rows.length, safePageSize);
  const start = (currentPage - 1) * safePageSize;
  return rows.slice(start, start + safePageSize);
}
