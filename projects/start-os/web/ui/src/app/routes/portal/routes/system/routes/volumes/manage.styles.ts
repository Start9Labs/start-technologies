export const SOURCE_MANAGE_STYLES = `
  .context {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-block-end: 1.5rem;
    font-weight: bold;

    tui-icon {
      font-size: 1.25rem;
    }
  }

  footer {
    position: sticky;
    bottom: 0;
    z-index: 10;
    display: flex;
    justify-content: flex-end;
    gap: 1rem;
    padding: 1rem 0;
    margin: 1rem -1px -1rem;
    background: var(--tui-background-elevation-1);
    box-shadow: inset 0 1px var(--tui-background-neutral-1);
  }

  .remove {
    margin-inline-end: auto;
  }
`
