import { createContext, useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useKeepAliveContext } from "keepalive-for-react";

type PageActionsTargets = {
  left: HTMLElement | null;
  right: HTMLElement | null;
};

export const PageActionsTarget = createContext<PageActionsTargets>({ left: null, right: null });

export function PageActions({ children, position = "right" }: { children: ReactNode; position?: "left" | "right" }) {
  const targets = useContext(PageActionsTarget);
  const { active } = useKeepAliveContext();
  const target = targets[position];
  return active && target ? createPortal(children, target) : null;
}
