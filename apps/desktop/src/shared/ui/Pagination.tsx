import controls from "./Controls.module.scss";
import { Icon } from "./Icon";
import { chevronDoubleLeftIcon, chevronDoubleRightIcon, chevronLeftIcon, chevronRightIcon } from "./icons";
import { Select } from "./Select";
import { TooltipTrigger } from "./TooltipTrigger";
import styles from "./Pagination.module.scss";

export function Pagination({ page, pageCount, pageSize, total, pageSizes = [20, 50, 100], onPageChange, onPageSizeChange }: {
  page: number;
  pageCount: number;
  pageSize: number;
  total: number;
  pageSizes?: number[];
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
}) {
  const disabledPrevious = page <= 1;
  const disabledNext = page >= pageCount;
  return <div className={styles.root}>
    <span>{t("共 {count} 条", { count: total })}</span>
    <div className={styles.controls}>
      <div className={styles.pageSize}><Select ariaLabel={t("每页条数")} value={String(pageSize)} options={pageSizes.map((size) => ({ value: String(size), label: t("{count} 条/页", { count: size }) }))} onChange={(value) => onPageSizeChange(Number(value))} /></div>
      <span>{t("第 {page} / {count} 页", { page, count: pageCount })}</span>
      <TooltipTrigger label={t("第一页")}><button className={controls.iconButton} aria-label={t("第一页")} disabled={disabledPrevious} onClick={() => onPageChange(1)}><Icon icon={chevronDoubleLeftIcon} size="1.1em" /></button></TooltipTrigger>
      <TooltipTrigger label={t("上一页")}><button className={controls.iconButton} aria-label={t("上一页")} disabled={disabledPrevious} onClick={() => onPageChange(page - 1)}><Icon icon={chevronLeftIcon} size="1.1em" /></button></TooltipTrigger>
      <TooltipTrigger label={t("下一页")}><button className={controls.iconButton} aria-label={t("下一页")} disabled={disabledNext} onClick={() => onPageChange(page + 1)}><Icon icon={chevronRightIcon} size="1.1em" /></button></TooltipTrigger>
      <TooltipTrigger label={t("最后一页")}><button className={controls.iconButton} aria-label={t("最后一页")} disabled={disabledNext} onClick={() => onPageChange(pageCount)}><Icon icon={chevronDoubleRightIcon} size="1.1em" /></button></TooltipTrigger>
    </div>
  </div>;
}
