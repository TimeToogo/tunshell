import styled, { css } from "styled-components";
import { COLOURS } from "../../theme/colours";

export const Wrapper = styled.div`
  width: 100%;
  height: 100%;
  color: ${COLOURS.OFF_WHITE};
  border-radius: 5px;
  box-shadow: 0 0 2px 1px #444 inset;
  position: relative;
  white-space: normal;
  word-break: break-word;
  line-height: 24px;
  font-size: 16px;
  padding: 15px 35px 15px 15px;
  box-sizing: border-box;
  margin: 0;

  &,
  & > span {
    background: ${COLOURS.TAN2};
  }

  &:not(:last-child) {
    margin-bottom: 15px;
  }
`;

export const Copy = styled.button`
  position: absolute;
  top: 5px;
  right: 5px;
  width: 30px;
  height: 30px;
  border-radius: 5px;
  display: flex;
  justify-content: center;
  align-items: center;
  background: none;
  border: none;
  font-size: 20px;
  color: ${COLOURS.OFF_WHITE};
  cursor: pointer;
  transition: 0.2s ease-out all;
  outline: 0 !important;

  &:hover {
    background: ${COLOURS.TAN4};
  }
`;

export const Copied = styled.div<{ active: boolean }>`
  position: absolute;
  background: ${COLOURS.TAN4};
  bottom: calc(100% + 5px);
  border-radius: 5px;
  font-size: 16px;
  color: ${COLOURS.TAN2};
  padding: 5px;
  pointer-events: none;
  transition: 0.2s ease-out all;
  opacity: 0;
  white-space: nowrap;

  ${({ active }) =>
    active &&
    css`
      opacity: 1;
    `}
`;
