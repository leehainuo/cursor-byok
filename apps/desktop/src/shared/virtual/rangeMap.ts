function clampIndex(index: number, min: number, max: number): number {
  return Math.min(Math.max(index, min), max)
}

export class RangeMap {
  private sizes: number[]
  private prefixSums: number[]

  constructor(sizes: number[]) {
    this.sizes = sizes.map((size) => Math.max(1, size))
    this.prefixSums = []
    this.rebuildPrefixSums()
  }

  get count(): number {
    return this.sizes.length
  }

  get totalSize(): number {
    return this.prefixSums[this.prefixSums.length - 1] ?? 0
  }

  reset(sizes: number[]): void {
    this.sizes = sizes.map((size) => Math.max(1, size))
    this.rebuildPrefixSums()
  }

  updateSize(index: number, size: number): number {
    if (index < 0 || index >= this.sizes.length) return 0

    const nextSize = Math.max(1, Math.ceil(size))
    const previousSize = this.sizes[index]
    if (previousSize === nextSize) return 0

    this.sizes[index] = nextSize
    this.rebuildPrefixSums(index)
    return nextSize - previousSize
  }

  sizeAt(index: number): number {
    return this.sizes[clampIndex(index, 0, this.sizes.length - 1)] ?? 0
  }

  positionAt(index: number): number {
    if (index <= 0) return 0
    if (index >= this.sizes.length) return this.totalSize
    return this.prefixSums[index - 1] ?? 0
  }

  indexAt(offset: number): number {
    if (this.sizes.length === 0) return 0
    if (offset <= 0) return 0
    if (offset >= this.totalSize) return this.sizes.length - 1

    let low = 0
    let high = this.prefixSums.length - 1
    while (low < high) {
      const mid = Math.floor((low + high) / 2)
      if (this.prefixSums[mid] > offset) high = mid
      else low = mid + 1
    }

    return low
  }

  indexAfter(offset: number): number {
    if (this.sizes.length === 0) return 0
    if (offset < 0) return 0
    if (offset >= this.totalSize) return this.sizes.length

    return Math.min(this.indexAt(offset) + 1, this.sizes.length)
  }

  private rebuildPrefixSums(startIndex = 0): void {
    if (startIndex <= 0 || this.prefixSums.length !== this.sizes.length) {
      this.prefixSums = new Array(this.sizes.length)
      startIndex = 0
    }

    let runningTotal = 0
    if (startIndex > 0) runningTotal = this.prefixSums[startIndex - 1] ?? 0

    for (let index = startIndex; index < this.sizes.length; index += 1) {
      runningTotal += this.sizes[index]
      this.prefixSums[index] = runningTotal
    }
  }
}
