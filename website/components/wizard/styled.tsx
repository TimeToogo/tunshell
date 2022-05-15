import styled from "styled-components";
import { COLOURS } from "../../theme/colours";
import { DropdownContainer } from "../dropdown/styled";

export const Wizard = styled.div`
  margin: 20px 0;
`;

export const Dialog = styled.div`
  padding: 20px 30px 30px 30px;
  background: ${COLOURS.TAN5};
  box-shadow: 0 0 5px #242424;
  border-radius: 5px;
  margin-bottom: 40px;
  position: relative;

  @media (max-width: 768px) {
    padding: 20px;
  }
`;

export const StepHeader = styled.header`
  font-size: 18px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  margin-bottom: 40px;

  &:last-child {
    margin-bottom: 0;
  }
`;

export const StepNumber = styled.span`
  display: inline-block;
  width: 18px;
  height: 18px;
  font-size: 12px;
  padding-left: 1px;
  display: flex;
  justify-content: center;
  align-items: center;
  border-radius: 50px;
  border: 1px solid #aaa;
  margin-bottom: 20px;
`;

export const Environments = styled.div`
  display: flex;
  flex-direction: row;

  @media (max-width: 900px) {
    flex-direction: column;
  }
`;

export const Environment = styled.div`
  flex: 0 0 calc(50% - 1px);
  padding: 0 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  box-sizing: border-box;

  h3 {
    font-size: 24px;
    font-weight: normal;
    text-align: center;
    margin: 0 0 5px 0;
    display: none;
  }

  p {
    margin: 0 0 20px 0;
    text-align: center;
  }

  @media (max-width: 900px) {
    &:not(:last-child) {
      margin-bottom: 30px;
    }
  }

  @media (max-width: 768px) {
    padding: 0;
  }
`;

export const Dropdown = styled.div`
  width: 350px;
  max-width: 100%;
  font-size: 24px;
  text-align: center;
`;

export const RelayLocation = styled.div`
  display: flex;
  margin-top: 15px;
  font-size: 16px;
  align-items: center;
  position: absolute;
  top: 0;
  right: 0;

  label {
    margin-right: 15px;
  }

  ${DropdownContainer} {
    min-width: 50px;
  }

  @media (max-width: 900px) {
    position: static;
    justify-content: center;
  }
`;

export const Separator = styled.hr`
  width: 1px;
  height: auto;
  background: #555;
  border: none;
  box-shadow: none;

  @media (max-width: 900px) {
    display: none;
  }
`;

export const Error = styled.p`
  margin-bottom: 0;

  &,
  & a {
    color: ${COLOURS.RED};
  }

  a {
    text-decoration: underline;
  }
`;

export const LaunchShell = styled.div`
  margin: auto 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
`;

export const LaunchShellLink = styled.a`
  text-align: center;
  color: white;
  margin-top: 10px;
  font-size: 12px;
`;
