export const BACKUP_LIST_STYLES = `
  .group {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 1.5rem 0 0.875rem;
  }

  h3 {
    margin: 0;
    font: var(--tui-typography-body-s);
    font-weight: bold;
    text-transform: uppercase;
    color: var(--tui-text-secondary);
  }

  [tuiCell] {
    margin-inline: -1rem;
    padding-inline: 1rem;
    border-radius: 0;

    &:has(+ [tuiCell]) {
      box-shadow: inset 0 -1px var(--tui-background-neutral-1);
    }
  }

  [tuiSubtitle] tui-icon {
    font-size: 1rem;
    vertical-align: text-bottom;
  }
`
