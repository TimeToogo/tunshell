import styled, { css } from "styled-components";
import { COLOURS } from "../../theme/colours";

export const DropdownContainer = styled.div<{ inline?: boolean; disabled?: boolean }>`
  .edd-root,
  .edd-root *,
  .edd-root *::before,
  .edd-root *::after {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  .edd-root {
    display: inline-block;
    position: relative;
    user-select: none;
    color: ${COLOURS.WHITE};
    width: 100%;
    background: ${COLOURS.TAN2};
    border-radius: 5px;
    padding: 0 10px;

    ${({ inline }) =>
      inline &&
      css`
        background: none;
        padding: 0;
        display: inline-flex;
      `}

    ${({ disabled }) =>
      disabled &&
      css`
        pointer-events: none;
        opacity: 0.5;
        cursor: default;
      `}
  }

  .edd-root-disabled {
    color: #ccc;
    cursor: not-allowed;
  }

  .edd-root.edd-root-invalid::after {
    background: rgb(255, 105, 105);
  }

  .edd-head {
    position: relative;
    overflow: hidden;
    transition: border-color 200ms;
    padding: 10px;

    ${({ inline }) =>
      inline &&
      css`
        padding: 0;
      `}
  }

  .edd-root:not(.edd-root-disabled) .edd-head:hover {
    border-bottom-color: #aaa;
  }

  .edd-value {
    width: 100%;
    display: inline-block;
    vertical-align: middle;

    ${({ inline }) =>
      inline &&
      css`
        text-decoration: underline;
      `}
  }

  .edd-arrow {
    position: absolute;
    width: 14px;
    height: 10px;
    top: calc(50% - 5px);
    right: 3px;
    transition: transform 150ms;
    pointer-events: none;
    color: #666;

    ${({ inline }) =>
      inline &&
      css`
        display: none;
      `}
  }

  .edd-root-disabled .edd-arrow {
    color: #ccc;
  }

  .edd-arrow::before {
    content: "";
    position: absolute;
    width: 8px;
    height: 8px;
    border-right: 2px solid currentColor;
    border-bottom: 2px solid currentColor;
    top: 0;
    right: 2px;
    transform: rotate(45deg);
    transform-origin: 50% 25%;
  }

  .edd-root-open .edd-arrow {
    transform: rotate(180deg);
  }

  .edd-value,
  .edd-option,
  .edd-group-label {
    white-space: nowrap;
    text-overflow: ellipsis;
    overflow: hidden;
  }

  .edd-root:not(.edd-root-disabled) .edd-value,
  .edd-option {
    cursor: pointer;
  }

  .edd-select {
    position: absolute;
    opacity: 0;
    width: 100%;
    left: -100%;
    top: 0;
  }

  .edd-root-native .edd-select {
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
  }

  .edd-body {
    opacity: 0;
    position: absolute;
    left: 0;
    right: 0;
    pointer-events: none;
    overflow: hidden;
    z-index: 999;
    box-shadow: 0 0 6px rgba(0, 0, 0, 0.08);
    border-top: 0;
    border-right: 0;
    background: ${COLOURS.OFF_BLACK};
  }

  .edd-root-open .edd-body {
    opacity: 1;
    pointer-events: all;
    transform: scale(1);
    transition: opacity 200ms, transform 100ms cubic-bezier(0.25, 0.46, 0.45, 0.94);
  }

  .edd-root-open-above .edd-body {
    bottom: 100%;
  }

  .edd-root-open-below .edd-body {
    top: 100%;
  }

  .edd-items-list {
    max-height: 0;
    transition: max-height 200ms cubic-bezier(0.25, 0.46, 0.45, 0.94);
    -webkit-overflow-scrolling: touch;
    border-radius: 5px;
    overflow: hidden;
    background: ${COLOURS.TAN2};
    margin-top: 3px;
  }

  .edd-items-list::-webkit-scrollbar {
    width: 12px;
  }

  .edd-items-list::-webkit-scrollbar-track {
    background: #efefef;
  }

  .edd-items-list::-webkit-scrollbar-thumb {
    background: #ccc;
  }

  .edd-group-label {
    font-size: 13px;
    padding: 4px 8px 4px 0;
    color: #555;
    font-weight: 600;
  }

  .edd-group-has-label {
    padding-left: 22px;
  }

  .edd-option {
    position: relative;
    padding: 10px 10px 10px 25px;

    ${({ inline }) =>
      inline &&
      css`
        padding: 5px;
      `}
  }

  .edd-option-selected {
    font-weight: 400;
  }

  .edd-option-focused:not(.edd-option-disabled) {
    color: ${COLOURS.TAN2};
    background: ${COLOURS.TAN4};
  }

  .edd-option-disabled,
  .edd-group-disabled .edd-option {
    cursor: default;
    color: #ccc;
  }

  .edd-gradient-top,
  .edd-gradient-bottom {
    content: "";
    position: absolute;
    left: 2px;
    right: 12px;
    height: 32px;
    background-image: linear-gradient(
      0deg,
      rgba(255, 255, 255, 0) 0%,
      rgba(255, 255, 255, 1) 40%,
      rgba(255, 255, 255, 1) 60%,
      rgba(255, 255, 255, 0) 100%
    );
    background-repeat: repeat-x;
    background-size: 100% 200%;
    pointer-events: none;
    transition: opacity 100ms;
    opacity: 0;
  }

  .edd-gradient-top {
    background-position: bottom;
    top: 0;
  }

  .edd-gradient-bottom {
    background-position: top;
    bottom: 0;
  }

  .edd-body-scrollable .edd-gradient-top,
  .edd-body-scrollable .edd-gradient-bottom {
    opacity: 1;
  }

  .edd-body-scrollable.edd-body-at-top .edd-gradient-top,
  .edd-body-scrollable.edd-body-at-bottom .edd-gradient-bottom {
    opacity: 0;
  }
`;
