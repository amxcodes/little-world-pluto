import React from 'react';

// Keep navigation mounted if a page fails, so the user can recover in place.
export class PageBoundary extends React.Component {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error, info) { console.error('Island page failed', error, info.componentStack); }
  render() {
    if (!this.state.failed) return this.props.children;
    return <section className="page-recovery" role="alert" data-layout-height={180}>
      <strong>This page couldn’t open</strong>
      <p>Try again or choose another page above.</p>
      <button type="button" onClick={() => this.setState({ failed: false })}>Try again</button>
    </section>;
  }
}
