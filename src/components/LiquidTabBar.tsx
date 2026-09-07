import type { ReactNode } from "react";

export type TabDefinition<Id extends string> = {
  id: Id;
  label: string;
  icon: ReactNode;
};

type LiquidTabBarProps<Id extends string> = {
  label: string;
  tabs: TabDefinition<Id>[];
  active: Id;
  onSelect: (id: Id) => void;
};

/**
 * Floating menu in the iOS liquid glass idiom: a translucent pill that blurs
 * and saturates whatever sits behind it, with a highlight sliding under the
 * active section.
 *
 * It sits above the safe area on a phone, and stays centred on a desktop
 * window at any width.
 */
export function LiquidTabBar<Id extends string>({
  label,
  tabs,
  active,
  onSelect,
}: LiquidTabBarProps<Id>) {
  const activeIndex = Math.max(
    0,
    tabs.findIndex((tab) => tab.id === active),
  );

  return (
    <nav className="tab-bar" aria-label={label}>
      <div className="tab-bar__glass" style={{ ["--tab-count" as string]: tabs.length }}>
        <span
          className="tab-bar__highlight"
          style={{ ["--active-index" as string]: activeIndex }}
          aria-hidden="true"
        />
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className="tab-bar__tab"
            onClick={() => onSelect(tab.id)}
            aria-current={tab.id === active ? "page" : undefined}
          >
            <span className="tab-bar__icon">{tab.icon}</span>
            <span className="tab-bar__label">{tab.label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}
